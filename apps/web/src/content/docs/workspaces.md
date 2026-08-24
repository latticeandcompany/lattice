---
title: Workspaces
description: Why Lattice makes you list the directories it runs tasks in, and what it can work out for itself once you have.
group: Concepts
order: 1
---

# Workspaces

A workspace is a directory with its own manifest. A JavaScript app with a
`package.json`, a Rust crate with a `Cargo.toml`, a Python package with a
`pyproject.toml`. Lattice runs tasks per workspace and caches results per
workspace, so the workspace is the unit that everything else in the tool is
counted in.

You list them under `workspaces` in the repo's `lattice.json`:

```json
{
  "workspaces": [
    { "name": "api", "path": "services/api" },
    { "name": "web", "path": "apps/web" }
  ]
}
```

That is the whole declaration for most workspaces. Everything else about them is
optional, and much of it Lattice can work out on its own.

## The list is explicit because a glob is a guess

`path` names one directory and is checked for existence exactly as written.
`packages/*` is read as a directory whose name is literally `packages/*`, and it
fails. There is no globbing and no filesystem crawl looking for manifests.

That is a real cost. A repo with forty packages under `packages/` gets forty
entries, and a new package means a new entry. The alternative costs more.
A crawl has to decide what counts as a package, and every repo has directories
that look like one and are not: a fixtures tree with its own `package.json`, a
vendored dependency, an example app nobody builds in CI. A tool that discovers
those runs tasks in them, caches results for them, and reports failures from
them. A tool that reads a list runs what the list says.

The list is also the thing you can read. `lattice.json` tells you what the repo
consists of without you running anything, and a diff to it is a reviewable
change rather than a side effect of adding a file.

## The workspace directory is a boundary, not a hint

Almost everything Lattice does to a task is scoped to that task's workspace
directory. The input walk stops there. `outputs` globs are resolved from there.
A cache hit clears what those globs match inside that directory before it
unpacks the stored artifact.

So a `path` has to stay inside the repo. An absolute path, or one that climbs
out with `..`, is rejected during validation rather than at the moment a task
would have written somewhere unexpected. So is a path that resolves out of the
repo. When Lattice discovers a workspace whose directory is a symlink to
somewhere else on the disk, it refuses that workspace, because the directory a
path resolves to is the one that bounds the task. A symlink that stays inside the
repo is still a workspace.

```text
Error: workspace path 'app' resolves to /elsewhere/app, which is outside the repo root. A workspace directory has to be inside the repo
```

Two workspaces also cannot share a name or resolve to the same directory, because
a name is a cache identity and a directory is a blast radius, and sharing either
would mean two tasks quietly overwriting each other's results.

The one thing that does not respect the boundary is a file above it. A base
`tsconfig.json` at the repo root is read by tasks in several workspaces and can
be named by no workspace-relative pattern. `globalDependencies` covers that case
at the repo level. See [Caching](/lattice/docs/caching).

## An `auto` workspace still has to show evidence

`auto` defaults to `true`, which means Lattice works out which tool runs the
workspace's tasks by looking at what is in the directory. Find `pnpm-lock.yaml`
and `pnpm run build` is the `build` command. Find `Cargo.lock` and it is
`cargo build`. You write no commands for either.

What `auto` does not mean is that Lattice will produce an answer no matter what
it finds. An empty directory stops the run before any task starts:

```text
Error: workspace 'empty' has an ambiguous or undeclared driver.
Lattice detected no driver. The directory holds no lockfile, no wrapper, and no native declaration.
Declare the driver in lattice.json, under this workspace:
  "auto": false, "scripts": { "build": "<command>" }
```

A directory holding nothing but a bare `package.json` stops it too, and says
what it saw:

```text
Error: workspace 'bare' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

This is the decision worth understanding about Lattice, because it is the one
that will interrupt you. A `package.json` proves the workspace is JavaScript. It
does not say whether `build` means `pnpm run build`, `yarn build`, or
`npm run build`, and those three are not interchangeable: they resolve different
dependency trees and read different lockfiles. Lattice could pick one. Picking
one means every repo that never chose a package manager silently gets whichever
Lattice's authors preferred, and the day it guesses wrong the failure looks like
a bug in your build rather than a decision in ours.

So a bare ecosystem marker is deliberately not enough. Lattice needs a file that
only one tool produces, or a file a developer wrote on purpose to name one:
`pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, a `packageManager` field, a
`.tool-versions`, a checked-in `gradlew`. Any of those settles it. Nothing does
not. [Driver detection](/lattice/docs/drivers) has the full ladder and what
happens when two tools both have a claim.

An auto workspace that resolves a driver is allowed to have no command for a
given task. Lattice skips it rather than failing, so a `test` task can exist in
the repo and cover only the workspaces that have tests.

## `auto: false` is the way out

Set `auto: false` and nothing is detected and nothing is inferred. You declare
`scripts` for every task the workspace should run:

```json
{
  "name": "utils",
  "path": "libs/utils",
  "auto": false,
  "scripts": {
    "build": "python3 build.py",
    "test": "python3 -m pytest"
  }
}
```

The trade you are making is precision for maintenance. An `auto: false`
workspace cannot be wrong about its driver, because it has none, and it cannot
be surprised by a lockfile someone adds later. It also cannot pick up a new task
for free: ask for a task it has no `scripts` entry for and the run fails rather
than skipping it, which is the correct behavior for a workspace that told you it
would declare everything.

Use it for a workspace whose build is a script rather than a tool, for a
workspace whose tool Lattice has never heard of, and for the case where
detection is ambiguous and you would rather write two lines than reason about
evidence rungs.

`scripts` also works without `auto: false`. An entry there always beats what
detection would infer, so an otherwise-auto workspace can name the one task no
driver could ever produce:

```json
{
  "name": "api",
  "path": "services/api",
  "scripts": { "deploy": "./deploy.sh" }
}
```

`build`, `test`, and `lint` still come from the detected driver. Only `deploy` is
declared.

## Per-workspace `engines` exist so one workspace can be different

A workspace's `engines` map merges over the root one, key by key. That is there
for the repo where one workspace genuinely needs a different tool version than
the rest, and where the honest fix is to say so in one place rather than raise
the floor for everything:

```json
{
  "engines": { "node": ">=20.0.0" },
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=26" } }
  ]
}
```

Every workspace but `web` is checked against Node 20 or newer. `web` needs 26.
Whether a constraint checks the host tool, installs one, or does neither depends
on the constraint's shape rather than its name, which is
[Engines and provisioning](/lattice/docs/engines).

## Workspace `dependsOn` does nothing by itself

This is the field people expect to order their build, and on its own it orders
nothing.

```json
{
  "workspaces": [
    { "name": "core", "path": "libs/core" },
    { "name": "api", "path": "services/api", "dependsOn": ["core"] }
  ],
  "tasks": {
    "build": { "dependsOn": ["^build"] }
  }
}
```

A workspace's `dependsOn` declares a fact about the repo: `api` uses `core`. It
does not declare a schedule. The schedule comes from the task, and only when the
task asks for it. Here `build` depends on `^build`, and the `^` means "in each
workspace this one depends on", so it reads `api`'s `dependsOn` and orders
`core:build` before `api:build`. Delete the `^build` and the two build in
whatever order the scheduler picks.

The separation is deliberate. Not every task should follow the dependency graph.
`lint` usually should not, because linting `core` has nothing to do with linting
`api`, and making every task inherit the workspace graph would serialize work
that has no reason to be serial. Declaring the relationship once and opting each
task into it is what lets `build` be ordered and `lint` be parallel in the same
repo. [Task graph](/lattice/docs/task-graph) covers both tokens.

A `dependsOn` name that matches no declared workspace is an error rather than a
no-op, because an unresolvable name builds no edge, and a missing edge is
invisible: the build still runs, in the wrong order, until it fails somewhere
unrelated.

## Where to look next

Types, defaults, and validation rules for every field on the workspace object
are in [Configuration](/lattice/docs/configuration). The full list of what
counts as driver evidence is in [Driver detection](/lattice/docs/drivers). Every
error message this page mentions is reproduced with its cause in
[Errors](/lattice/docs/errors).
