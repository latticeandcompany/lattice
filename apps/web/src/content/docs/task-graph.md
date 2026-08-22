---
title: Task graph
description: Why one task name becomes many nodes, how the two dependsOn tokens differ, and why the whole graph is built before anything runs.
group: Concepts
order: 2
---

# Task graph

`lattice run build` does not run a command called `build`. There is no such
command. `build` is a name you chose, and running it means asking every
workspace in the repo what `build` means to it, linking the answers by
dependency, and running the independent ones at the same time.

A four-workspace repo turns one word into four processes. A forty-workspace repo
turns it into forty. Understanding how that expansion happens is most of
understanding what Lattice does.

## A task name is a question, not a command

The keys under `tasks` in `lattice.json` are names. For each one you request,
Lattice asks every resolved workspace whether it has a command for that name. An
`auto` workspace answers from its detected driver, so `build` in a pnpm
workspace becomes `pnpm run build`. An `auto: false` workspace answers from its
`scripts` map and nowhere else.

A workspace with an answer becomes one node, written `workspace:task`, holding
the resolved command. A workspace without one is either skipped or an error,
depending on which kind it is. An `auto` workspace is skipped silently, because
"my driver has no `test` script" is a normal thing for a workspace to say. An
`auto: false` workspace halts the run, because it promised to declare everything
and a missing declaration is a mistake rather than an absence. See
[Workspaces](/lattice/docs/workspaces) for both cases.

This is why a task name means something slightly different in every workspace
and the same thing across the repo. `lattice run test` means "test everything
that has tests" without you maintaining a list of which workspaces those are.

## The two `dependsOn` tokens answer different questions

Two fields in `lattice.json` are called `dependsOn` and they do not mean the same
thing. A *workspace*'s `dependsOn` names other workspaces. A *task*'s
`dependsOn` names other tasks, in one of two forms:

| Token | Reads as | Edge it creates |
| --- | --- | --- |
| `task` (bare) | that task, in this same workspace | `workspace:task` to `workspace:this-task`, for every workspace that has both |
| `^task` | that task, in each workspace this one depends on | `depWorkspace:task` to `workspace:this-task`, following the workspace's own `dependsOn` |

Those are the only two forms. There is no glob and no `workspace#task`
addressing, which means you cannot write a dependency that names one specific
workspace's task. That looks like a gap until you try to use it. A rule that
names a workspace only holds until someone renames the workspace, and a
`tasks` map full of them stops being a description of how work fits together and
becomes a hand-maintained schedule. The two tokens say "after my own X" and
"after my dependencies' X", and between them they express the shape almost every
repo actually has.

The token after `^` does not have to repeat the current task's name, though it
nearly always does. `"build": { "dependsOn": ["^build"] }` reads as "build every
dependency first", which is the rule you want for a compiled language and the
rule you usually do not want for `lint`.

A task with no `dependsOn` has no incoming edges, so every workspace's node for
it is a root and they all start at once.

## Worked example

Four workspaces. `core` depends on nothing, `api` depends on `core`, `web`
depends on `api`, and `docs` stands alone:

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

`--dry-run` prints every node in one valid dependency order and runs nothing:

```text
❖ lattice  dry run · build
  → docs:build  echo build docs
  → core:build  echo build core
  → api:build  echo build api
  → web:build  echo build web
```

Running it shows what that order buys. `docs:build` and `core:build` have no
prerequisites, so they start together and finish together. `api:build` waits on
`core:build`, and `web:build` waits on `api:build`:

```text
core:build: running
docs:build: running
docs:build: done (0.01s)
core:build: done (0.01s)
api:build: running
api:build: done (0.00s)
web:build: running
web:build: done (0.00s)
lattice: 4 tasks, 0 cached, 0 failed, 0.02s
```

Ask for `test` instead and the four `build` nodes come along, because `test`
depends on the bare `build` and every workspace has both:

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

You asked for one task in a four-workspace repo and got eight nodes. That is the
expansion doing its job.

## Naming several tasks builds one graph, not several runs

`lattice run lint test build` collects the transitive closure of all three task
names first, then builds a single graph from the whole thing. A node that two of
the requested tasks both need exists once. Here `test` depends on `build` and
`build` is also requested directly, so each workspace's `build` node appears
exactly once and `lint` parallelizes around it wherever the graph has room.

The alternative would be three runs back to back, which is what `--sequentially`
gives you when you want it. That builds a separate full graph per requested task
and finishes each before starting the next. Inside a phase, tasks still run in
dependency order and in parallel. Only the phases stop overlapping.

Each phase's graph is built fresh, so a later phase can reintroduce a node an
earlier phase already ran:

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

In a real run those reintroduced nodes come back as cache hits rather than
repeated work, which is the reason `--sequentially` costs less than it looks
like it should. Reach for it when phase order matters for a reason outside the
graph: a lint gate you want to fail before anything compiles, or a CI log you
want readable one stage at a time.

## Parallelism comes from a semaphore, not from the graph

The graph says what *may* run at once. It does not say how much does. Lattice
keeps a set of nodes whose prerequisites have all finished and spawns them all,
bounded by a semaphore that defaults to the number of logical CPUs.

Separating the two is what keeps the bound adjustable without touching your
config. `--concurrency N` caps it at `N`, which is the flag you want on a CI box
where the reported CPU count is the host's rather than your container's, or on a
laptop where you would like to keep a browser responsive.
`--concurrency 0` is ignored rather than treated as unbounded, because a run
with no bound at all is a fork bomb with a friendly name. The default takes over
instead.

As each node finishes, its dependents get closer to ready, and any that reach
zero outstanding prerequisites join the next pass. On a failure, Lattice spawns
nothing further and reports the first failure. Under `--continue` it marks every
transitive dependent of the failed node as skipped and keeps going with the rest,
then exits non-zero. Flag syntax is in
[Selecting what runs](/lattice/docs/filtering).

## The whole graph is validated before anything starts

Lattice builds and topologically sorts the entire graph before it spawns a
single process. Every structural problem is therefore found while nothing has
run and nothing has been written:

- A cycle, direct or through a chain, is rejected.
- A requested task with no entry under `tasks` fails, and the error lists the
  tasks that do exist.
- A `dependsOn` naming a task the `tasks` map never defines fails, with a
  suggestion when the name is close to a real one. The `^` form is checked
  against the same map, so `^build` still requires `build` to be defined.
- A [persistent](/lattice/docs/persistent-tasks) task with a dependent is
  rejected, because nothing can be scheduled after a process that never exits.

The last three are worth the up-front check for the same reason. A dependency
Lattice cannot resolve would build no edge, and a missing edge does not announce
itself: the run succeeds, in the wrong order, until something downstream fails
for a reason that has nothing to do with the typo that caused it. Failing at
graph construction turns a mystery into a message.

Every one of those messages, with its exact text and its cause, is in
[Errors](/lattice/docs/errors).

## Where to look next

Narrowing a run to part of the graph is
[Selecting what runs](/lattice/docs/filtering). The fields on a task, including
`timeout`, are in [Configuration](/lattice/docs/configuration). What happens to
a node once it is scheduled is [Caching](/lattice/docs/caching).
