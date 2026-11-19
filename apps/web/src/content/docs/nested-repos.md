---
title: Wrap a repo that has its own task runner
description: Bring a subtree with its own task graph in as one workspace.
group: Guides
order: 3
---

# Wrap a repo that has its own task runner

Some subtrees already have a task runner, a lockfile, and an internal dependency
graph of their own. You do not have to flatten one into individual Lattice
workspaces to bring it into the graph. List its directory, and Lattice runs it
as one workspace through the runner it already has, scheduled and cached like any
other.

This guide works through `examples/nested-repo` in the Lattice repo.

## The shape

```text
lattice.json           two workspaces: frontend, api
frontend/              an inner monorepo: turbo.json, package.json workspaces, a lockfile
  packages/ui/         built first (an inner dependency)
  packages/site/       depends on ui, emits dist/bundle.js
services/api/          a directory of shell scripts that serves site's bundle
```

## The config

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "2.0.0",
  "workspaces": ["frontend", "services/api"],
  "overrides": {
    "frontend": {
      "tasks": {
        "build": {
          "outputs": ["packages/*/dist/**"],
          "ignore": ["**/node_modules/**", "**/.turbo/**"]
        }
      }
    },
    "services/api": {
      "name": "api",
      "dependsOn": ["frontend"],
      "tasks": {
        "build": "mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh",
        "test": "test -x dist/serve.sh",
        "serve": { "run": "sh dist/serve.sh", "dependsOn": ["build"], "cache": false },
        "clean": "rm -rf dist"
      }
    }
  }
}
```

## Why `frontend` needs no commands

`frontend` is listed in `workspaces`, so it is one workspace. Its own
`package.json` names `packages/*` as workspaces, but only the repo root's
workspace files are expanded, so Lattice never sees `ui` or `site` individually.

Its `turbo.json` makes turbo its driver, and every task in that file becomes a
task Lattice can run there: `build`, `test` and `clean` run as `turbo run build`
and so on. `turbo` itself is found in `frontend/node_modules/.bin`, which Lattice
puts on the task's `PATH`, so it comes from the inner repo's own install
(`cd frontend && npm install`) and the root pins nothing.

`api` is the opposite case: a directory with no manifest at all. Its override
writes each command out and gives it a name, because a path-keyed override is how
you reach a workspace whose directory is its only identity.

## Cover the whole subtree

Lattice hashes `frontend` as one unit with no view inside it. With no `inputs`,
every file in the workspace that git does not ignore is hashed, which catches a
change anywhere. The `ignore` list keeps out what changes without the source
changing:

| Ignored | Why |
| --- | --- |
| `**/node_modules/**` | The installed tree. The lockfile is hashed instead |
| `**/.turbo/**` | The inner runner's own cache. It changes on every run, which would make every build a miss |

`outputs` names every artifact directory the inner repo produces. A workspace
driven by turbo has no default `build` output, because the runner could write
anywhere, so it has to be said: `packages/*/dist/**`. A hit restores exactly
those paths.

## Two caches, not one

Wrapping a subtree does not disable its inner cache. It adds one above it. A
Lattice hit restores every inner package's `dist/` in one step without starting
the inner runner. A miss hands the whole workspace back to the runner, which
decides package by package what reruns. The first build after a change inside
`frontend` costs one narrowed inner rebuild; the second costs nothing.

## Run it

With the inner repo's dependencies installed once, from the repo root:

```text
$ lattice run build --dry-run
❖ lattice  dry run · build
  → frontend:build  turbo run build
  → api:build  mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh
```

`api` waits for `frontend` because the override says it depends on it, and
`build` depends on `^build`. A second run comes back from cache for both. Edit
`frontend/packages/ui/src/index.js` and both rerun: `frontend` because its inputs
moved, and `api` because its prerequisite's key did.

## A task only one workspace has

`serve` exists on `api` alone. `lattice run serve` runs it there, and `frontend`
sits it out. `serve` depends on `build`, so `frontend:build` and `api:build` run
first, from cache when nothing changed, and `api` never serves a bundle that was
not built.

`serve` is one of the names Lattice treats as a long-running server, so the run
streams and waits for it to exit. This example's `serve.sh` prints the bundle and
exits, which Lattice reports as `api:serve: exited (code 0)`. A real server would
keep running until `Ctrl-C`. See [Persistent tasks](/lattice/docs/persistent-tasks).

## Wrap, or flatten?

Wrap the subtree as one workspace when it has its own runner and internal graph
that the rest of the repo does not need to schedule, and when its packages are
built and tested together.

Flatten it — make its members workspaces, by adding them to the root's workspace
file or to `workspaces` — when another workspace needs to depend on one specific
inner package, or when you want a rebuild narrowed to the one package that
changed. Flattening trades the one-line handoff for per-package caching and
edges at Lattice's own resolution.
