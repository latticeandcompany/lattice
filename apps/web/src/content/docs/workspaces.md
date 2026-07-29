---
title: Workspaces
description: The project directory Lattice runs and caches tasks in, and what you declare about it.
group: Concepts
order: 1
---

# Workspaces

A workspace is one project directory: a single unit of task running and
caching. A JavaScript app, a Rust crate, a Python package, a Go service — each
one is a workspace. `lattice.json` declares the full list under `workspaces`,
and every root task is expanded across all of them into one execution graph
(see [Task graph](/lattice/docs/task-graph)).

```json
{
  "workspaces": [
    { "name": "api", "path": "services/api" },
    { "name": "web", "path": "apps/web" }
  ]
}
```

`name` and `path` are the only required fields. Everything else — `auto`,
`engines`, `dependsOn`, `scripts` — has a default and is covered below.

## Finding the root

Every Lattice command looks for `lattice.json` by walking up from the current
directory, so you can run `lattice` from any directory inside the repo, not
just the root. Once the root is found, its `lattice.json` is parsed and
validated — duplicate workspace names and empty paths are rejected before
anything else runs. Every workspace `path` is then resolved relative to that
root, never to the directory you happened to run `lattice` from.

## `path` is literal — never a glob

A workspace's `path` is a single directory, checked for existence as-is. It is
not a pattern, and Lattice does not expand it. If you write a wildcard, it is
treated as a literal (and almost certainly nonexistent) directory name:

```json
{ "name": "packages", "path": "packages/*" }
```

```text
workspace path 'packages/*' does not point to a directory; workspace paths
are literal directories, not globs
```

That is the exact failure Lattice raises for any `path` that isn't a real
directory. Two workspaces also can't share a name or resolve to the same
directory — both are duplicate errors, not a merge. List one entry per
project directory instead.

## `auto`: inferred vs. explicit

`auto` defaults to `true`. An auto workspace has its task driver — the tool
that actually runs its tasks — resolved from evidence in the directory: a
lockfile, a `packageManager` field, a `Cargo.lock`, and so on. That resolution
is [driver detection](/lattice/docs/drivers); read that page for the full
evidence ladder. Once a driver is resolved, Lattice infers a command for every
root task the driver or its manifest supports (`pnpm run build`, `cargo test`,
…) — you don't write those commands yourself.

Set `auto: false` to opt a workspace out of all of it:

```json
{
  "name": "utils",
  "path": "libs/utils",
  "auto": false,
  "scripts": {
    "build": "python3 build.py",
    "test": "python3 -m pytest",
    "lint": "python3 -m py_compile src/*.py",
    "clean": "rm -rf dist"
  }
}
```

With `auto: false`, nothing is detected and nothing is inferred: you declare
`scripts` for every root task this workspace should run, and `engines` for
any toolchain constraint it needs. If a root task is requested and this
workspace has no matching entry in `scripts`, the run fails rather than
silently skipping it:

```text
workspace 'utils' is "auto": false but declares no command for task 'build';
add it under this workspace's "scripts" map in lattice.json
```

An auto workspace, by contrast, simply sits out a task it has no driver
support for — it isn't required to cover every task in the pipeline.

## `engines`: per-workspace toolchain constraints

A workspace's `engines` map merges with the root `engines`, and the
workspace's entries win per key — a workspace can tighten or replace a
root-level constraint without touching the rest of the repo:

```json
{
  "engines": { "node": ">=20.0.0" },
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=26" } }
  ]
}
```

Here every other workspace validates against Node ≥20, but `web` requires
≥26. What an engine constraint's shape does — trust `PATH`, validate a
version, or provision a pinned install — is the [Engines and
provisioning](/lattice/docs/engines) page.

## `dependsOn`: ordering workspaces

A workspace's `dependsOn` names other workspaces (by `name`) it depends on:

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

On its own, `dependsOn` declares nothing about ordering — it only takes effect
where a task's own `dependsOn` uses a `^`-prefixed name. Here, `api`'s `build`
depends on `^build`, which resolves through `api`'s `dependsOn: ["core"]` to
`core`'s `build`: `core` builds before `api`. Without a task naming `^build`
(or whichever task), declaring `dependsOn` on the workspace has no scheduling
effect at all. The full distinction between a bare task name and a
`^`-prefixed one is the [Task graph](/lattice/docs/task-graph) page.

## `scripts`: your commands, always

`scripts` maps a task name straight to a shell command, and an entry there
always wins over anything an auto workspace would otherwise infer:

```json
{
  "name": "api",
  "path": "services/api",
  "dependsOn": ["core"],
  "scripts": {
    "deploy": "./deploy.sh"
  }
}
```

`api` here is still `auto: true` (the default): `build`, `test`, and `lint`
are still inferred from its detected driver, but `deploy` — which no driver
could invent — is exactly the command declared. You only need to name the
tasks you want to override or supply; everything else falls back to
detection. For a workspace with `auto: false`, `scripts` is the only source
of commands there is.

The exhaustive field-by-field reference — types, defaults, and every other
`lattice.json` key — is [Configuration](/lattice/docs/configuration).
