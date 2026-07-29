---
title: Task graph
description: How Lattice expands a task across workspaces and orders the result.
group: Concepts
order: 2
---

# Task graph

When you run `lattice run build`, Lattice does not just run `build` once. It
expands `build` into one node per workspace that defines it, links those nodes
by dependency, and runs the independent ones in parallel. This page covers how
that graph is built, how `lattice run lint test build` merges several tasks
into one graph, and what `--sequentially` changes about that.

## From a task name to nodes

`build`, `test`, `lint` — the keys under `tasks` in `lattice.json` — are names
you choose, not commands. For a requested task, Lattice asks every workspace
one question: does this workspace have a command for it? An `auto` workspace
answers from its detected driver; a workspace with `"auto": false` answers only
from its `scripts` map (see [Workspaces](/lattice/docs/workspaces) and
[Driver detection](/lattice/docs/drivers) for how a command is resolved).

- If the workspace has a command, it becomes one node: `workspace:task`,
  holding the resolved command.
- If an `auto` workspace has no matching command, it is skipped silently —
  not every workspace has to run every task.
- If a `"auto": false` workspace has no matching command, the run halts:

  ```text
  Error: workspace 'docs' is "auto": false but declares no command for task
  'test'; add it under this workspace's "scripts" map in lattice.json
  ```

## `dependsOn`: two tokens, two things they combine

Two separate `dependsOn` fields exist, and the task graph is built from both:

- A **workspace**'s `dependsOn` names other *workspaces* it depends on.
- A **task**'s `dependsOn` names other *tasks*, using one of two tokens.

| Token | Reads as | Edge it creates |
| --- | --- | --- |
| `task` (bare) | "that task, in this same workspace" | `workspace:task` → `workspace:this-task`, for every workspace that has both |
| `^task` | "that task, in each workspace this one depends on" | `depWorkspace:task` → `workspace:this-task`, following that workspace's own `dependsOn` list |

These are the only two forms the graph builder understands (see
`build_execution_graph_multi` in `crates/dagger/src/lib.rs`) — there is no
glob or `workspace#task` addressing. The token after `^` does not have to
repeat the current task's name, but in practice it almost always does: a
`build` task depending on `^build` means "build every dependency first,"
which is what the schema's own description says. A task with an empty or
absent `dependsOn` has no incoming edges at all — every workspace's node for
it is independent.

A workspace with no `dependsOn` contributes no `^`-edges anywhere; its nodes
are graph roots for every task. Filtering the workspace set with
`--filter` (see [Selecting what runs](/lattice/docs/filtering)) has the same
effect on excluded workspaces: they contribute no nodes, so any `^`-edge that
would have pointed at one is silently absent, not an error.

## A worked example

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
means: `docs:build` and `core:build` have no prerequisites, so they start
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

`lattice run test --dry-run` pulls in `build` transitively (`test` depends on
`build`, `build` depends on `^build`), so the same four `build` nodes appear
ahead of the four `test` nodes:

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

`lattice run lint test build` does not run three separate graphs back to
back. Every requested task's transitive task-name closure is collected first,
then all of it — every task, every workspace that defines it — is built into
one combined `DiGraph`. A dependency two stacked tasks have in common is one
node, not two: `test` depends on `build`, and `build` is also requested
directly, so each workspace's `build` node appears exactly once and both
`lint` and the rest of the graph parallelize around it wherever the graph
allows.

## `--sequentially`: one graph per task, in order

`--sequentially` turns that one merged graph into a separate full graph per
requested task, run start-to-finish before the next begins — the tasks
themselves still run in dependency order and in parallel *within* each phase,
only the phases no longer overlap. Because each phase's graph is built fresh
from that one task's own transitive `dependsOn`, a later phase can reintroduce
nodes an earlier phase already ran:

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
❖ lattice  dry run · build (phase)
  → docs:build  echo build docs
  → core:build  echo build core
  → api:build  echo build api
  → web:build  echo build web
```

The `test` phase's graph includes `build` nodes even though a prior phase
never ran here; if it had, those `build` tasks would already be cache hits
(see [Caching](/lattice/docs/caching)) rather than real work. A failing phase
stops the whole run before the next phase starts, unless you pass
`--continue`, in which case Lattice records the failure and moves on to the
next phase, then exits non-zero if any phase failed.

## Parallel execution and its bound

The graph itself is presentation- and scheduler-agnostic: `build_schedule`
flattens it into an in-degree count per node, plus each node's prerequisites
and dependents. The runner keeps a "ready" set of nodes whose in-degree has
reached zero and spawns all of them at once, bounded by a semaphore — that
bound, not the graph, is what limits parallelism. It defaults to the number of
logical CPUs (`std::thread::available_parallelism`); `--concurrency N` caps it
at `N` instead. `--concurrency 0` is not an unbounded mode — zero is ignored
and the default takes over. Full flag syntax, alongside
`--filter` and `--continue`, is covered in
[Selecting what runs](/lattice/docs/filtering).

As each node finishes, its dependents' in-degree drops by one; a dependent
reaching zero joins the ready set on the next scheduling pass. When a task
fails and the run is not in `--continue` mode, no further nodes are spawned
and the first failure is reported. In `--continue` mode, every transitive
dependent of the failed node is marked skipped instead of run, since a
prerequisite it needed can no longer succeed.

## Cycles and other graph-construction errors

Before anything runs, Lattice topologically sorts the whole graph once. A
cycle — two tasks depending on each other, directly or through a chain — is
rejected up front, before a single command starts:

```text
Error: cycle detected in task dependency graph
```

Two related rules are enforced before a task ever starts:

- A requested task that has no entry under `tasks` in `lattice.json` fails
  immediately, listing the tasks that do exist:

  ```text
  Error: task 'nope' is not defined in lattice.json; available tasks: a, b
  ```

- A [persistent](/lattice/docs/persistent-tasks) task can never be a
  prerequisite — it streams until stopped, so it never reaches "done" for a
  dependent to wait on:

  ```text
  Error: persistent task 'dev' in workspace 'app' cannot be depended on by
  other tasks
  ```
