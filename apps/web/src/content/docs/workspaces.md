---
title: Workspaces
description: The project directory Lattice runs and caches tasks in, and what you declare about it.
group: Concepts
order: 1
---

# Workspaces

A workspace is one project directory — a JavaScript app, a Rust crate, a Python
package, a Go service — and it is the unit of task running and caching.
`lattice.json` lists them all under `workspaces`, and every root task is
expanded across all of them into one execution graph
(see [Task graph](/lattice/docs/task-graph)).

```json
{
  "workspaces": [
    { "name": "api", "path": "services/api" },
    { "name": "web", "path": "apps/web" }
  ]
}
```

`name` and `path` are required. `auto`, `engines`, `dependsOn`, and `scripts`
each have a default.

## Finding the root

Every command walks up from the current directory looking for `lattice.json`, so
you can run `lattice` from anywhere inside the repo. The config is parsed and
validated before anything else runs — duplicate workspace names and empty paths
are rejected there — and every workspace `path` resolves relative to the root,
not to the directory you ran from.

## `path` is literal, never a glob

A `path` is a single directory, checked for existence as written. A wildcard is
treated as a literal directory name and fails:

```json
{ "name": "packages", "path": "packages/*" }
```

```text
workspace path 'packages/*' does not point to a directory; workspace paths
are literal directories, not globs
```

Two workspaces cannot share a name or resolve to the same directory; either is a
duplicate error, not a merge. List one entry per project directory.

A `path` also has to stay inside the repo. An absolute path, or one that climbs
out with `..`, is rejected:

```text
workspace 'esc' has a path '../outside' that points outside the repo root;
workspace paths must stay inside the repo
```

The workspace directory is the boundary for everything downstream — which files
are hashed, which the `outputs` globs match, and which a cache hit clears before
unpacking — so a path that leaves the repo would put all three somewhere Lattice
has no business writing.

## `auto`: inferred or explicit

`auto` defaults to `true`. An auto workspace has its task driver — the tool that
runs its tasks — resolved from evidence in the directory: a lockfile, a
`packageManager` field, a `Cargo.lock`. Lattice then infers a command for every
root task that driver or its manifest supports (`pnpm run build`, `cargo test`),
so you write none of them. See [Driver detection](/lattice/docs/drivers) for the
evidence ladder.

`auto: false` turns all of that off:

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

Nothing is detected and nothing is inferred. Declare `scripts` for every root
task the workspace should run, and `engines` for any toolchain constraint it
needs. Requesting a root task the workspace has no `scripts` entry for fails the
run:

```text
workspace 'utils' is "auto": false but declares no command for task 'build';
add it under this workspace's "scripts" map in lattice.json
```

An auto workspace instead sits out a task its driver has no command for. It is
not required to cover every task in the pipeline.

## `engines`: per-workspace toolchain constraints

A workspace's `engines` map merges with the root `engines`, and the workspace's
entries win per key, so a workspace can tighten or replace a root constraint
without touching the rest of the repo:

```json
{
  "engines": { "node": ">=20.0.0" },
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=26" } }
  ]
}
```

Every workspace but `web` validates against Node ≥20; `web` requires ≥26. What a
constraint's shape does — trust `PATH`, validate a version, or provision a
pinned install — is [Engines and provisioning](/lattice/docs/engines).

## `dependsOn`: ordering workspaces

A workspace's `dependsOn` names other workspaces, by `name`:

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

On its own it changes no ordering. It takes effect only where a task's own
`dependsOn` uses a `^`-prefixed name. Here `build` depends on `^build`, which
resolves through `api`'s `dependsOn: ["core"]` to `core`'s `build`, so `core`
builds before `api`. With no task naming `^build`, the workspace `dependsOn` has
no scheduling effect at all. Bare versus `^`-prefixed task names are covered in
[Task graph](/lattice/docs/task-graph).

Every name has to match a declared workspace. A name that matches nothing builds
no edge, which would leave the two workspaces building in whatever order the
scheduler picked — so it is rejected instead:

```text
Error: workspace 'api' depends on 'cor', which is not a declared workspace
Did you mean `core`?
Declared workspaces: core, api
```

## `scripts`: declaring commands directly

`scripts` maps a task name to a shell command, and an entry there always wins
over anything an auto workspace would infer:

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

`api` is still `auto: true`, so `build`, `test`, and `lint` still come from its
detected driver; only `deploy` — which no driver could infer — is the declared
command. Name just the tasks you want to override or supply. Under
`auto: false`, `scripts` is the only source of commands.

Field-by-field types and defaults for every `lattice.json` key are in
[Configuration](/lattice/docs/configuration).
