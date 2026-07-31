---
title: Selecting what runs
description: Narrow a run to one workspace, list several tasks, or change how the graph executes.
group: Concepts
order: 6
---

# Selecting what runs

`lattice run` always starts from the full task graph for the tasks you name —
see [Task graph](/lattice/docs/task-graph) for how that graph is built. The
flags below change which part of it executes, and how.

## Filtering to one workspace

`-f`/`--filter <PATTERN>` selects the workspaces whose **name** contains
`PATTERN` and runs the tasks you named there, along with everything those
workspaces depend on:

```sh
lattice run build --filter core
```

It matches the `name` field, not `path`, and it is a substring match rather than
a glob: `--filter co` matches `core` and `docs-core` alike. Only one `--filter`
is accepted per run; passing it twice is a clap error, not two patterns:

```text
error: the argument '--filter <PATTERN>' cannot be used multiple times
```

A filter that matches nothing is a clean no-op, not a failure. Running
`lattice run build --filter nonexistent` prints
`lattice: no workspaces matched filter 'nonexistent'.` and exits `0`.

### Dependencies come along

The matched workspaces are the roots of the run, not all of it. A `^build` edge
pointing at a workspace the pattern did not match still resolves to that
workspace's `build`, and so does that workspace's own `^build`, all the way down.
Take this repo's own `lattice.json`, where `lattice-runner` depends on `dagger`,
`lattice-cache`, `lattice-output`, `lattice-config`, and `lattice-workspace`:

```sh
$ lattice run build --dry-run --filter lattice-runner
❖ lattice  dry run · build
  → lattice-output:build (dependency)  cargo build
  → lattice-config:build (dependency)  cargo build
  → lattice-cache:build (dependency)  cargo build
  → lattice-workspace:build (dependency)  cargo build
  → dagger:build (dependency)  cargo build
  → lattice-runner:build  cargo build
```

`(dependency)` marks a node that is in the graph because something the pattern
matched needs it. Only `lattice-runner:build` matched. The other five run first,
in dependency order, and each one whose inputs haven't changed comes back from
cache. A filtered run costs a cache lookup per prerequisite, not a rebuild.

The edges only travel one way. Nothing that *depends on* a match is included:
`--filter dagger` leaves out `lattice-runner` and `lattice` even though both
depend on `dagger`, and `--filter lattice-output` runs one node, since it depends
on nothing.

A workspace pulled in this way is only asked for the tasks its dependents need.
An `auto: false` workspace outside the filter with no `scripts` entry for the task
you named does not halt the run the way it would if the filter had matched it.

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
Windows). Top-to-bottom is one order the scheduler would honor, not a timeline:
independent branches still run concurrently in a real run. Without
`--sequentially`, stacked tasks print as one merged graph under a single banner;
with it, each task gets its own banner and topological order, one `(phase)` per
named task, in the order you listed them. `--dry-run` composes with `--filter`
and prints the same graph a real run would execute, `(dependency)` tags and all.

## Keeping going after a failure with `--continue`

By default, one task failing stops the run: nothing still queued starts, and
anything already running is left to finish or fail on its own. `--continue`
instead keeps starting everything that does not depend, transitively, on the
task that failed.

```sh
lattice run build --continue
```

A task downstream of a failure is never started. It is marked skipped with
`dependency failed` as the reason and counted separately from tasks that
actually failed. Everything outside that downstream slice runs as it would have
without the flag. Either way, `lattice run` exits non-zero if anything failed.

## Capping parallelism with `--concurrency`

`--concurrency N` caps how many tasks the scheduler runs at once. Without it,
the cap is the number of logical CPUs available on the machine:

```sh
lattice run test --concurrency 4
```

The cap applies across the whole run, not per workspace: with `--concurrency 1`,
two independent tasks still run one at a time, in whichever order the scheduler
picks them up. It does not change the graph — dependency order is still honored,
and a task still starts only once everything it depends on has finished or
restored from cache. `--concurrency 0` is treated the same as not passing the
flag at all.

## Naming several tasks in one run

`lattice run` takes one or more task names:

```sh
lattice run lint test build
```

By default these merge into a single combined graph before anything runs, so a
dependency shared by more than one named task appears once and runs once; see
[Stacked tasks share one graph](/lattice/docs/task-graph#stacked-tasks-share-one-graph).
The same de-duplication applies to the dependencies a `--filter` pulls in: a
workspace two matches both depend on gets one node.

## Running each task to completion with `-s`/`--sequentially`

`-s`/`--sequentially` turns that merging off. Each named task gets its own
graph, and each graph runs to completion before the next starts:

```sh
lattice run lint test build --sequentially
```

For `lint test build`, that means every `lint` node first, then everything
`test` needs (which pulls in `build` again, from cache if nothing changed), then
everything `build` needs on its own. Use it when a later task must never race a
concurrent instance of an earlier one — `clean` before `build`, for instance.

A failure during one phase stops the run before the next phase starts, the same
fail-fast behavior as above. With `--continue`, a failed phase is recorded and
`--sequentially` moves on regardless:

```sh
$ lattice run lint build --sequentially --continue --no-cache -l
lattice: running `lint` across 1 workspace(s)
app:lint: FAILED
lattice: 1 tasks, 0 cached, 1 failed, 0.00s
lattice: running `build` across 1 workspace(s)
app:build: done (0.00s)
lattice: 1 tasks, 0 cached, 0 failed, 0.00s
```

`build` has no dependency on `lint`, so nothing marks it as downstream of the
failure: within each phase, `--continue` still only skips tasks that actually
depend on the one that failed. The run exits non-zero once every phase has run.
