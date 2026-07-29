---
title: Selecting what runs
description: Narrow a run to one workspace, list several tasks, or change how the graph executes.
group: Concepts
order: 6
---

# Selecting what runs

`lattice run` always starts from the full task graph for the tasks you name —
see [Task graph](/lattice/docs/task-graph) for how that graph is built. This
page covers the flags that change what part of it actually executes, and how
it executes: `--filter`, `--dry-run`, `--continue`, `--concurrency`, naming
several tasks at once, and `--sequentially`.

## Filtering to one workspace

`-f`/`--filter <PATTERN>` keeps only the workspaces whose **name** contains
`PATTERN`, before the task graph is built:

```sh
lattice run build --filter core
```

It matches against the workspace's `name` field, not its `path`, and it's a
substring match, not a glob — `--filter co` matches a workspace named `core`
just as well as one named `docs-core`. Only one `--filter` is accepted per
run; passing it twice is a clap error, not two patterns:

```text
error: the argument '--filter <PATTERN>' cannot be used multiple times
```

If nothing matches, that's a clean no-op, not a failure — `lattice run build
--filter nonexistent` prints `lattice: no workspaces matched filter
'nonexistent'.` and exits `0`.

### Filtering does not pull in dependencies

This is the part worth reading carefully. Filtering removes non-matching
workspaces from the set *before* the dependency graph is built, so an edge
into a filtered-out workspace is never created — it isn't run first, and it
isn't an error either. Take this repo's own `lattice.json`, where
`lattice-runner` depends on `dagger`, `lattice-cache`, `lattice-output`,
`lattice-config`, and `lattice-workspace`:

```sh
$ lattice run build --dry-run --filter lattice-runner
❖ lattice  dry run · build
  → lattice-runner:build  cargo build
```

Every one of `lattice-runner`'s dependencies is gone from the graph, and
`lattice-runner:build` runs on its own — against whatever those dependencies
last produced, cached or not. Compare the unfiltered graph:

```sh
$ lattice run build --dry-run
❖ lattice  dry run · build
  → web:build  npm run build
  → lattice-output:build  cargo build
  → lattice-config:build  cargo build
  → lattice-cache:build  cargo build
  → lattice-workspace:build  cargo build
  → dagger:build  cargo build
  → lattice-runner:build  cargo build
  → lattice:build  cargo build
```

Use `--filter` to re-run one workspace's task in isolation once you know its
dependencies are already up to date; don't reach for it as a shortcut for "and
everything it needs," because that isn't what it does. If you want the
dependency closure too, drop `--filter` and let the full graph run — a cache
hit makes the already-current dependencies cheap, not repeated work.

## Previewing with `--dry-run`

`--dry-run` prints the resolved task graph in topological order and exits
without running or caching anything:

```sh
$ lattice run lint check build --sequentially --dry-run
❖ lattice  dry run · lint (phase)
  → lattice:lint  cargo lint
  → lattice-runner:lint  cargo lint
  ...
❖ lattice  dry run · check (phase)
  → lattice:check  cargo check
  ...
❖ lattice  dry run · build (phase)
  → web:build  npm run build
  ...
```

Each line is `workspace:task` followed by the exact resolved shell command for
that node — the same command the runner would hand to `sh -c` (or `cmd /C` on
Windows). Reading top to bottom shows the order the scheduler would honor,
though independent branches can still run concurrently in a real run; nothing
is guaranteed to happen at the same wall-clock moment just because two lines
are adjacent. Without `--sequentially`, stacked tasks print as one merged
graph under a single banner; with it, each task gets its own banner and its
own topological order, one "(phase)" per named task, in the order you listed
them. `--dry-run` composes with `--filter`: the graph it prints is the same
narrowed graph a real run would execute.

## Keeping going after a failure with `--continue`

By default, one task failing stops the run: nothing still queued starts, and
anything already running is left to finish or fail on its own, but no new work
begins. `--continue` changes that to: keep starting everything that doesn't
depend, transitively, on the task that failed.

```sh
lattice run build --continue
```

A task that depends — directly or transitively — on a failed task is never
started; it's marked skipped, with `dependency failed` as the reason, and
counted separately from tasks that actually failed. Everything outside that
downstream slice runs exactly as it would have without `--continue`. Either
way, `lattice run` exits non-zero if anything failed; `--continue` only
changes how much of the graph you get to see fail (or pass) in one invocation
instead of finding out about the second problem on your next run.

## Capping parallelism with `--concurrency`

`--concurrency N` caps how many tasks the scheduler runs at once. Without it,
the cap is the number of logical CPUs available on the machine:

```sh
lattice run test --concurrency 4
```

The cap applies across the whole run, not per workspace — with `--concurrency
1`, two tasks that don't depend on each other still run one at a time, in
whichever order the scheduler picks them up, never interleaved. It does not
change the graph itself: dependency order is still honored, and a task still
only starts once every task it depends on has finished (or restored from
cache). `--concurrency 0` is treated the same as not passing the flag at all.

## Naming several tasks in one run

`lattice run` takes one or more task names:

```sh
lattice run lint test build
```

By default these are merged into a single combined graph before anything
runs. A dependency shared by more than one of the named tasks — `test`
depending on `build`, say — appears once in that graph and runs once, and
independent tasks like `lint` parallelize alongside the rest instead of
waiting their turn. This is why the earlier `lattice-runner` example only
prints one `build` node even though several stacked roots could in principle
need it.

## Running each task to completion with `-s`/`--sequentially`

`-s`/`--sequentially` turns that merging off. Instead of one combined graph,
each named task gets its own graph, and each graph runs to completion before
the next one starts:

```sh
lattice run lint test build --sequentially
```

For `lint test build`, that means: build and run every `lint` node to
completion, then build and run everything `test` needs (which pulls in
`build` again, from cache if nothing changed), then finally everything
`build` needs on its own. Reach for `--sequentially` when a later task's
result should never race a concurrent instance of an earlier one — for
example, `clean` before `build`, where `clean` running concurrently with a
`build` task in another workspace would be a bug in your task definitions
that stacking would otherwise mask, not fix.

By default, a failure during one phase stops the run before the next phase
starts, the same fail-fast behavior described above. Combine it with
`--continue` and that stops being true across phases too — a failed phase is
recorded, but `--sequentially` moves on to the next phase regardless:

```sh
$ lattice run lint build --sequentially --continue --no-cache -l
lattice: running `lint` across 1 workspace(s)
app:lint: FAILED
lattice: 1 tasks, 0 cached, 1 failed, 0.00s
lattice: running `build` across 1 workspace(s)
app:build: done (0.00s)
lattice: 1 tasks, 0 cached, 0 failed, 0.00s
```

`build` here has no dependency on `lint`, so nothing marks it as downstream of
the failure — within each phase, `--continue` still only skips tasks that
actually depend on the one that failed. The run exits non-zero once every
phase has run, because at least one of them failed.
