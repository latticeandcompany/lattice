---
title: Wrap a repo that has its own task runner
description: Bring a subtree with its own task graph in as one workspace.
group: Guides
order: 3
---

# Wrap a repo that has its own task runner

Some subtrees already have a task runner, a lockfile, and an internal dependency
graph of their own. You do not have to flatten one into individual Lattice
workspaces to bring it into the graph. Declare it as a single workspace whose
`scripts` shell out to the runner it already has, and it becomes one node
Lattice schedules and caches like any other.

This guide works through `examples/nested-repo` in the Lattice repo. Every block
below was captured against a copy of it.

## The shape

```text
lattice.json           two workspaces: frontend, api
frontend/              an inner monorepo with its own task runner and lockfile
  packages/ui/         built first (an inner dependency)
  packages/site/       depends on ui, emits dist/bundle.js
services/api/          reads site's bundle at run time
```

`frontend` wraps a repo with its own inner ordering, `ui` before `site`. `api`
reads what `frontend` produced.

## Declare the wrapper

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    {
      "name": "frontend",
      "path": "frontend",
      "auto": false,
      "scripts": {
        "build": "sh -c 'if [ -x node_modules/.bin/turbo ]; then node_modules/.bin/turbo run build; else echo \"[frontend] turbo is not installed — run: (cd frontend && npm install)\" >&2; exit 1; fi'",
        "test": "sh -c 'if [ -x node_modules/.bin/turbo ]; then node_modules/.bin/turbo run test; else echo \"[frontend] turbo is not installed — run: (cd frontend && npm install)\" >&2; exit 1; fi'",
        "clean": "sh -c 'rm -rf .turbo packages/*/dist && echo \"frontend clean complete\"'"
      }
    },
    {
      "name": "api",
      "path": "services/api",
      "auto": false,
      "dependsOn": ["frontend"],
      "scripts": {
        "build": "mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh && echo 'api built'",
        "test": "sh -c 'test -x dist/serve.sh && echo \"api test ok\"'",
        "serve": "sh dist/serve.sh",
        "clean": "sh -c 'rm -rf dist && echo \"api clean complete\"'"
      }
    }
  ],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["**/*"],
      "ignore": ["**/node_modules/**", "**/.turbo/**", "**/dist/**"],
      "outputs": ["dist/**", "packages/*/dist/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["**/*"],
      "ignore": ["**/node_modules/**", "**/.turbo/**", "**/dist/**"]
    },
    "serve": {
      "dependsOn": ["build"],
      "cache": false
    },
    "clean": {
      "cache": false
    }
  },
  "settings": {
    "maxCacheSize": "1GB"
  }
}
```

Nothing here names the inner packages, and the root declares no `engines`. The
runner Lattice invokes for `frontend:build` comes from that repo's own
`node_modules`, installed the ordinary way for its ecosystem
(`cd frontend && npm install`).

## Set `auto: false` on the wrapper

Both workspaces set `"auto": false`, which skips driver detection and runs only
what `scripts` declares. A wrapped subtree wants that, because the handoff is
specific: which runner binary, invoked from where, with which flags. The script
above calls `node_modules/.bin/turbo` rather than whatever `turbo` resolves to
on `PATH`, and prints an install hint when that binary is missing. Detection has
no way to produce either detail.

A manual workspace needs a script for any task you run directly against it.
`frontend` has no `serve` script, because it has nothing to serve. Running
`serve` repo-wide, or filtered to `api`, works. Running it scoped to `frontend`
fails to resolve. A task pulled in only as a dependency is skipped where a
workspace has no command for it.

## Cover the whole subtree with `inputs`

The inner runner tracks its own file dependencies and keeps its own cache. That
changes what your `inputs` and `outputs` need to cover, not whether you declare
them. Lattice hashes `frontend` as one unit with no view into `ui` or `site`
individually, so `inputs` has to catch a change anywhere inside:

```json
{
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["**/*"],
      "ignore": ["**/node_modules/**", "**/.turbo/**", "**/dist/**"],
      "outputs": ["dist/**", "packages/*/dist/**"]
    }
  }
}
```

`**/*` is wide because the wrapped repo owns its internal layout. The precision
lives in `ignore`:

| Ignored | Why |
| --- | --- |
| `**/node_modules/**` | The installed tree. The lockfile is hashed instead. See [Caching](/lattice/docs/caching). |
| `**/.turbo/**` | The inner runner's own cache directory. It changes on every run regardless of source, which would make every build a miss. |
| `**/dist/**` | Build output. An output glob left inside `inputs` feeds yesterday's build into today's key. |

`outputs` has to name every artifact directory the wrapped repo produces, at
every level: `dist/**` for `api`, `packages/*/dist/**` for each inner package.
Lattice restores exactly those paths on a hit and no others.

## Two caches, not one

Wrapping a subtree does not disable its inner cache. It adds a second one above
it. Lattice's cache covers `frontend` as a whole, and a hit restores every inner
package's `dist/` in one step without invoking the inner runner at all. The
inner cache covers individual packages, and it only comes into play on a Lattice
miss, when Lattice hands the whole workspace back to its runner and the runner
decides package by package what reruns.

So the first build after a change inside `frontend` costs one inner rebuild,
narrowed by the inner cache to the package that changed. The second costs
nothing. This is the trade the wrapper buys: Lattice's cache is coarse and
language-agnostic, which is what lets the rest of the repo treat `frontend` as
one node.

## Run it

With the inner repo's dependencies installed once
(`cd frontend && npm install`), from the repo root:

```text
$ lattice run build
frontend:build: running
frontend:build: done (1.43s)
api:build: running
api:build: done (0.01s)
lattice: 2 tasks, 0 cached, 0 failed, 1.46s
```

Again with nothing changed, and both workspaces come back from cache without the
inner runner executing:

```text
$ lattice run build
frontend:build: cache hit [176b1a72]
api:build: cache hit [25020aef]
lattice: 2 tasks, 2 cached, 0 failed, 0.01s
lattice: full cache, nothing to run
```

Edit a file anywhere under `frontend`, say `packages/ui/src/index.js`, and both
workspaces rerun. `frontend` misses because its `inputs` changed, and `api`
misses because a task's cache key includes the keys of its prerequisites:

```text
$ lattice run build
frontend:build: running
frontend:build: done (0.54s)
api:build: running
api:build: done (0.01s)
lattice: 2 tasks, 0 cached, 0 failed, 0.56s
```

To see which of the two reasons applied, add `-v`. The miss line names the
component that moved, `inputs changed` for `frontend` and
`dependencies changed` for `api`. See [Caching](/lattice/docs/caching).

## Check the handoff before it runs

`--dry-run` prints the resolved commands and executes nothing, which is how to
read a handoff script before it fires:

```text
$ lattice run build --dry-run
❖ lattice  dry run · build
  → frontend:build  sh -c 'if [ -x node_modules/.bin/turbo ]; then node_modules/.bin/turbo run build; else echo "[frontend] turbo is not installed — run: (cd frontend && npm install)" >&2; exit 1; fi'
  → api:build  mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh && echo 'api built'
```

## Run a task only the wrapper's dependent has

`serve` exists on `api` alone, so filter to it. `serve` depends on `build`, so
the filter still pulls `frontend:build` in and `api` never serves a bundle that
was not built:

```text
$ lattice run serve --filter api -v
lattice: running `build+serve` across 2 workspaces
lattice: frontend:build: hash 4a12ab5673815fee
frontend:build: cache hit [4a12ab56]
lattice: api:build: hash e29eccac6575e5f2
api:build: cache hit [e29eccac]
lattice: api:serve: hash 31369803a034db9a
api:serve: running
api:serve: api serving:
api:serve: module.exports = {"site":"site@1","button":"ui/button@1"};
api:serve: done (0.01s)
lattice: 3 tasks, 2 cached, 0 failed, 0.01s
```

See [Selecting what runs](/lattice/docs/filtering).

## Wrap, or flatten?

Wrap the subtree as one workspace when:

- it has its own task runner, lockfile, and internal dependency graph, and the
  rest of the repo does not need to schedule that graph
- its packages are only ever built or tested together, never targeted
  individually from outside the subtree
- adopting it incrementally matters more than exposing its internals. See
  [Adopting Lattice](/lattice/docs/adopting-lattice).

Flatten it into individual workspaces when another workspace needs to depend on
one specific inner package, when you want a rebuild narrowed to the single inner
package that changed, or when the inner runner is the only thing standing
between you and running one package's tests directly. Flattening trades the
one-line handoff for per-package caching and dependency edges at Lattice's own
resolution.
