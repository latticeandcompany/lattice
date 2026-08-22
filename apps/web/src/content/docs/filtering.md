---
title: Selecting what runs
description: Narrow a run to one workspace, stack several tasks, or change how the graph executes.
group: Concepts
order: 6
---

# Selecting what runs

`lattice run` always starts from the full task graph for the tasks you name. The
flags here change which part of it executes, and how. For how the graph is
built, see [Task graph](/lattice/docs/task-graph).

## Narrow the run to one workspace

`-f`/`--filter <PATTERN>` selects the workspaces whose `name` contains
`PATTERN`, and runs the tasks you named there plus everything those workspaces
depend on:

```sh
lattice run build --filter core
```

It matches `name`, not `path`, and it is a substring match rather than a glob:
`--filter co` matches `core` and `docs-core` alike. Only one `--filter` is
accepted per run:

```text
error: the argument '--filter <PATTERN>' cannot be used multiple times
```

A filter that matches nothing is a clean no-op. `lattice run build --filter
nonexistent` prints `lattice: no workspaces matched filter 'nonexistent'.` and
exits `0`.

### Dependencies come along

The matched workspaces are the roots of the run, not all of it. A `^build` edge
pointing at a workspace the pattern did not match still resolves to that
workspace's `build`, and so on down. In this repo's own `lattice.json`,
`lattice-runner` depends on five other crates:

```text
$ lattice run build --dry-run --filter lattice-runner
❖ lattice  dry run · build
  → lattice-events:build (dependency)  cargo build
  → lattice-config:build (dependency)  cargo build
  → lattice-cache:build (dependency)  cargo build
  → lattice-workspace:build (dependency)  cargo build
  → dagger:build (dependency)  cargo build
  → lattice-runner:build  cargo build
```

`(dependency)` marks a node that is in the graph because something the pattern
matched needs it. Only `lattice-runner:build` matched. The other five run first,
in dependency order, and each one whose inputs have not changed comes back from
cache. A filtered run costs a cache lookup per prerequisite, not a rebuild.

The edges travel one way only. Nothing that depends on a match is included:

```text
$ lattice run build --dry-run --filter dagger
❖ lattice  dry run · build
  → lattice-config:build (dependency)  cargo build
  → lattice-workspace:build (dependency)  cargo build
  → dagger:build  cargo build
```

`lattice-runner` and `lattice` are both left out, even though both depend on
`dagger`.

A workspace pulled in this way is only asked for the tasks its dependents need.
An `auto: false` workspace outside the filter, with no `scripts` entry for the
task you named, does not halt the run the way it would if the filter had matched
it.

## Preview a run with `--dry-run`

`--dry-run` prints the resolved task graph in topological order and exits
without running or caching anything:

```text
$ lattice run lint build --dry-run
❖ lattice  dry run · lint build
  → app:lint  exit 1
  → lib:lint  echo lint lib
  → lib:build  echo build lib
  → app:build  echo build app
```

Each line is `workspace:task` followed by the exact resolved shell command,
which is what the runner would hand to `sh -c`, or to `cmd /C` on Windows.
Top-to-bottom is one order the scheduler would honor, not a timeline:
independent branches still run concurrently in a real run.

`--dry-run` composes with `--filter` and prints the same graph a real run would
execute, `(dependency)` tags and all.

## Keep going after a failure

By default one failure stops the run: nothing still queued starts, and anything
already running is left to finish or fail on its own. `--continue` instead keeps
starting everything that does not depend, transitively, on the task that failed.

```sh
lattice run build --continue
```

A task downstream of a failure is never started. It is marked skipped with
`dependency failed` as the reason, and counted separately from tasks that
actually failed. Everything outside that downstream slice runs as it would have
without the flag. Either way, `lattice run` exits `1` if anything failed.

## Cap parallelism

`--concurrency N` caps how many tasks the scheduler runs at once. Without it the
cap is the number of logical CPUs the machine reports:

```sh
lattice run test --concurrency 4
```

The cap applies across the whole run, not per workspace. With
`--concurrency 1`, two independent tasks still run one at a time, in whichever
order the scheduler picks them up. It does not change the graph: dependency
order is still honored, and a task starts only once everything it depends on has
finished or restored from cache. `--concurrency 0` is treated the same as not
passing the flag.

## Name several tasks in one run

`lattice run` takes one or more task names:

```sh
lattice run lint test build
```

By default these merge into a single graph before anything runs, so a dependency
shared by more than one named task appears once and runs once. See [Stacked
tasks share one
graph](/lattice/docs/task-graph#stacked-tasks-share-one-graph). The same
de-duplication applies to the dependencies a `--filter` pulls in: a workspace
that two matches both depend on gets one node.

## Run each task to completion before the next

`-s`/`--sequentially` turns that merging off. Each named task gets its own
graph, and each graph runs to completion before the next starts:

```text
$ lattice run lint build --sequentially --dry-run
❖ lattice  dry run · lint (phase)
  → app:lint  exit 1
  → lib:lint  echo lint lib
❖ lattice  dry run · build (phase)
  → lib:build  echo build lib
  → app:build  echo build app
```

Each phase gets its own banner and its own topological order, in the order you
listed the tasks. Reach for this when a later task must never race a concurrent
instance of an earlier one, such as `clean` before `build`.

A failure during one phase stops the run before the next phase starts, the same
fail-fast behavior as above. With `--continue`, a failed phase is recorded and
the next phase runs anyway:

```text
$ lattice run lint build --sequentially --continue --no-cache -l
lattice: running `lint` across 2 workspaces
lattice: lib:lint: hash df4442eb2e737292
lattice: app:lint: hash 5a5af3015e55e870
app:lint: running
lib:lint: running
lib:lint: lint lib
app:lint: FAILED
lib:lint: done (0.00s)
lattice: 2 tasks, 0 cached, 1 failed, 0.01s
lattice: running `build` across 2 workspaces
lattice: lib:build: hash 7ae387a086f8bae5
lib:build: running
lib:build: build lib
lib:build: done (0.00s)
lattice: app:build: hash da221351319e0a06
app:build: running
app:build: build app
app:build: done (0.00s)
lattice: 2 tasks, 0 cached, 0 failed, 0.01s
```

`build` has no dependency on `lint`, so nothing marks it as downstream of the
failure. Within each phase, `--continue` still only skips tasks that depend on
the one that failed. The run exits `1` once every phase has run.
