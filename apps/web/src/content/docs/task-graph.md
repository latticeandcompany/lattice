---
title: Task graph
description: Why one task name becomes many nodes, how the two dependsOn tokens differ, and why the whole graph is built before anything runs.
group: Concepts
order: 2
---

# Task graph

`lattice run build` does not run a command called `build`. It asks every
workspace in the repo what `build` means to it, links the answers by dependency,
and runs the independent ones at the same time.

A four-workspace repo turns one word into four processes. A forty-workspace repo
turns it into forty. Understanding that expansion is most of understanding what
Lattice does.

## A task name is a question, not a command

For each task you request, Lattice asks every workspace whether it has a command
for that name: a script its manifest declares, a verb its tool has, or a command
an override writes out. `build` in a pnpm workspace becomes `pnpm run build`; in
a Cargo crate, `cargo build`; in a Gradle build, `./gradlew build`. See [Driver
detection](/lattice/docs/drivers#what-a-driver-can-run) for where each answer
comes from.

A workspace with an answer becomes one node, written `workspace:task`, holding
the resolved command. A workspace without one sits the run out. "This package
has no `test` script" is a normal thing for a workspace to say, so `lattice run
test` means "test everything that has tests" without a list of which those are.

No task has to be declared in `lattice.json` to run. The `tasks` map there says
how a task *behaves* — what it depends on, what it caches — in every workspace
that has it.

## The two `dependsOn` tokens

A task's `dependsOn` names other tasks, in one of two forms:

| Token | Reads as | Edge it creates |
| --- | --- | --- |
| `task` (bare) | that task, in this same workspace | `ws:task` before `ws:this-task`, in every workspace that has both |
| `^task` | that task, in each workspace this one depends on | `dep:task` before `ws:this-task`, following the workspace's dependencies |

Which workspaces a workspace depends on comes from its manifest: a
`workspace:*` dependency in `package.json`, a `path` dependency in `Cargo.toml`,
a `require` in `go.mod`. An edge the manifests cannot state, such as one between
languages, goes in `overrides.<ws>.dependsOn`. See
[Workspaces](/lattice/docs/workspaces#dependencies-come-from-the-manifest-too).

There is no glob and no `workspace#task` addressing. A rule that names a
specific workspace holds until someone renames it, and a `tasks` map full of
them becomes a hand-maintained schedule. "After my own X" and "after my
dependencies' X" express the shape almost every repo has.

`build` depends on `^build` without being told, because building a package
before the packages it imports is the rule every ecosystem agrees on. Nothing
else inherits the workspace graph: `lint` has no `dependsOn`, so every
workspace's `lint` starts at once.

## Worked example

An npm monorepo of four packages. `api` depends on `core`, `web` depends on
`api`, and `docs` stands alone, all stated in their `package.json` files. The
only line in `lattice.json` makes tests wait for their own build:

```json
{
  "tasks": { "test": { "dependsOn": ["build"] } }
}
```

`--dry-run` prints every node in one valid order and runs nothing:

```text
❖ lattice  dry run · build
  → docs:build  npm run build
  → core:build  npm run build
  → api:build  npm run build
  → web:build  npm run build
```

Running it shows what that order buys. `docs:build` and `core:build` have no
prerequisites, so they start together. `api:build` waits on `core:build`, and
`web:build` on `api:build`:

```text
docs:build: running
core:build: running
docs:build: done (0.61s)
core:build: done (0.61s)
api:build: running
api:build: done (0.10s)
web:build: running
web:build: done (0.10s)
lattice: 4 tasks, 0 cached, 0 failed, 0.87s
```

Ask for `test` and the four `build` nodes come along, because `test` depends on
the bare `build`:

```text
❖ lattice  dry run · test
  → docs:build  npm run build
  → core:build  npm run build
  → api:build  npm run build
  → web:build  npm run build
  → web:test  npm run test
  → docs:test  npm run test
  → core:test  npm run test
  → api:test  npm run test
```

One task in a four-workspace repo, eight nodes. That is the expansion doing its
job.

## Naming several tasks builds one graph

`lattice run lint test build` collects everything the three names need first,
then builds a single graph. A node two of them share exists once: `test` depends
on `build` and `build` is also requested, so each `build` node appears exactly
once, and `lint` runs alongside wherever there is room.

`--sequentially` builds a separate graph per task and finishes each before
starting the next:

```text
❖ lattice  dry run · lint (phase)
  → web:lint  npm run lint
  → docs:lint  npm run lint
  → core:lint  npm run lint
  → api:lint  npm run lint
❖ lattice  dry run · test (phase)
  → docs:build  npm run build
  ...
```

A later phase can include a node an earlier one ran; in a real run it comes back
from cache. Reach for it when phase order matters for a reason outside the
graph: a lint gate that should fail before anything compiles, or a CI log you
want readable one stage at a time.

## Running from inside a workspace

A run started inside a workspace directory is scoped to that workspace and
everything it depends on, the same closure `--filter` produces. From
`packages/web`, `lattice run build` builds `core`, `api` and `web`, and leaves
`docs` alone. Run from the root for everything. See [Selecting what
runs](/lattice/docs/filtering).

## Parallelism comes from a semaphore

The graph says what *may* run at once. Lattice keeps the set of nodes whose
prerequisites have all finished and runs them, bounded by a semaphore that
defaults to the number of logical CPUs. `--concurrency N` caps it without
touching your config; `--concurrency 0` means the default rather than no bound.

As each node finishes, its dependents move closer to ready. On a failure,
Lattice starts nothing new and reports the first failure. Under `--continue` it
skips every node that depends on the failed one and keeps going with the rest,
then exits non-zero.

## The whole graph is checked before anything starts

Lattice builds and sorts the entire graph before it spawns a process, so every
structural problem is found while nothing has run:

- A task no workspace has is refused, with the tasks that do exist.
- A `dependsOn` naming a task no workspace has is refused, with the nearest
  name. A dependency that resolved to nothing would build no edge, and a missing
  edge does not announce itself: the run succeeds, in the wrong order, until
  something downstream fails for a reason unrelated to the typo.
- A [persistent](/lattice/docs/persistent-tasks) task with a dependent is
  refused, because nothing can come after a process that never exits.
- A cycle is refused, naming a node on it. Workspace edges come from manifests,
  so two packages that list each other as dev dependencies are a cycle here.

Every message, with its cause, is in [Errors](/lattice/docs/errors).

## Where to look next

Narrowing a run is [Selecting what runs](/lattice/docs/filtering). Every task
field, including `timeout` and the built-in defaults, is in
[Configuration](/lattice/docs/configuration#tasks). What happens to a node once
it is scheduled is [Caching](/lattice/docs/caching).
