---
title: Nested repos
description: Wrapping a subtree that already has its own task runner.
group: Guides
order: 3
---

# Nested repos

Some subtrees are a project of projects, already wired together by a task runner
of their own — a JS monorepo with its own task graph, or a Gradle multi-module
build. You do not have to flatten one into individual Lattice workspaces to bring
it into the graph. Declare it as a single workspace whose `scripts` shell out to
the runner it already has, and it becomes one node Lattice schedules and caches
like any other.

This page walks through `examples/nested-repo`; the output below is from running
it.

## The shape

```text
lattice.json           two workspaces: frontend, api
frontend/              an inner monorepo with its own task runner and lockfile
  packages/ui/         built first (inner dependency)
  packages/site/       depends on ui, emits dist/bundle.js
services/api/          reads site's bundle at run time
```

`frontend` wraps a repo with its own inner dependency graph (`ui` before `site`).
`api` reads `frontend`'s output. The root `lattice.json`:

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

Nothing here names the inner packages, and the root declares no `engines`: the
runner Lattice invokes for `frontend:build` comes from that repo's own
`node_modules`, installed the ordinary way for its ecosystem
(`cd frontend && npm install`). See [Workspaces](/lattice/docs/workspaces) for
`path`, `dependsOn`, and `scripts` in general.

## `auto: false` on the wrapper

Both workspaces set `"auto": false`. An `auto` workspace runs the evidence ladder
and infers commands from the driver it detects; a manual workspace skips detection
and runs only what you wrote in `scripts`.

A wrapped subtree wants the manual form, because the handoff command is specific:
which runner binary, invoked from where, with which flags. In this example the
script calls `node_modules/.bin/turbo` rather than whatever `turbo` resolves to on
`PATH`, and prints an install hint when that binary is missing. Inference has no
way to produce that.

A manual workspace needs a script for any task you run directly against it.
`frontend` has no `serve` script, since it has nothing to serve. Running `serve`
scoped to `frontend` fails to resolve; running it repo-wide, or filtered to `api`,
works — a task pulled in only as a dependency is skipped where a workspace doesn't
apply.

## `inputs` and `outputs` are still yours to declare

The inner runner tracks its own file dependencies and keeps its own cache. That
changes what Lattice's `inputs` and `outputs` need to cover, not whether you
declare them. Lattice hashes `frontend` as one opaque unit with no visibility into
`ui` or `site` individually, so its `inputs` glob has to catch a change anywhere
inside:

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
| `**/node_modules/**` | The installed tree. The package manager's lockfile is hashed instead — see [Caching](/lattice/docs/caching). |
| `**/.turbo/**` | The inner runner's own cache directory. It changes on every run regardless of source, making every build a miss. |
| `**/dist/**` | Build output. An output glob left inside `inputs` feeds yesterday's build into today's cache key. |

`outputs` has to name every artifact directory the wrapped repo produces, at every
level — `dist/**` for `api`, `packages/*/dist/**` for each inner package. Lattice
restores exactly those paths on a hit and no others.

## Two caches, not one

Wrapping a subtree does not disable its inner cache; it adds a second one above
it. Lattice's cache covers `frontend` as a whole, and a hit restores every inner
package's `dist/` in one step without invoking the inner runner at all. The inner
cache covers individual packages, and only comes into play on a Lattice miss, when
`frontend`'s hash has changed and Lattice hands the whole workspace back to its
runner; from there the inner runner decides package by package what reruns.

So the first build after a change inside `frontend` costs one inner rebuild,
narrowed by the inner cache to the package that changed, and the second costs
nothing. Lattice's cache is coarse and language-agnostic, which is what lets the
rest of the repo treat `frontend` as one node; the inner one is fine-grained and
specific to that ecosystem.

## Running it

With the inner repo's dependencies installed once (`cd frontend && npm install`),
from the repo root:

```sh
$ lattice run build
frontend:build: running
frontend:build: done (0.66s)
api:build: running
api:build: done (0.01s)
lattice: 2 tasks, 0 cached, 0 failed, 0.68s
```

Again with nothing changed, and both workspaces come back from cache without the
inner runner executing:

```sh
$ lattice run build
frontend:build: cache hit [587c2274]
api:build: cache hit [11b8de60]
lattice: 2 tasks, 2 cached, 0 failed, 0.01s
```

Edit a file anywhere under `frontend` — say `packages/ui/src/index.js` — and only
`frontend` misses. `api` still waits on `frontend` completing, but its own inputs
haven't changed:

```sh
$ lattice run build
frontend:build: running
frontend:build: done (0.52s)
api:build: cache hit [11b8de60]
lattice: 2 tasks, 1 cached, 0 failed, 0.52s
```

`--dry-run` shows the resolved commands without running them, which is how to
check the handoff script before it executes:

```sh
$ lattice run build --dry-run
❖ lattice  dry run · build
  → frontend:build  sh -c 'if [ -x node_modules/.bin/turbo ]; then node_modules/.bin/turbo run build; else echo "[frontend] turbo is not installed — run: (cd frontend && npm install)" >&2; exit 1; fi'
  → api:build  mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh && echo 'api built'
```

`serve` is filtered to `api`, since `frontend` has nothing to serve (`--filter` is
covered in [Selecting what runs](/lattice/docs/filtering)):

```sh
$ lattice run serve --filter api -l
lattice: running `build+serve` across 1 workspace(s)
api:build: cache hit [11b8de60]
api:serve: running
api:serve: api serving:
api:serve: module.exports = {"site":"site@1","button":"ui/button@1"};
api:serve: done (0.01s)
lattice: 2 tasks, 1 cached, 0 failed, 0.01s
```

## Don't copy an upstream artifact at build time

A workspace's cache key covers only its own declared `inputs`. It does not fold in
the keys of the workspaces it depends on, and an `inputs` glob cannot reach outside
its own workspace directory. If `api:build` copied `frontend`'s bundle into its own
`dist/`, a `frontend`-only edit would rebuild `frontend` and still hand `api` a
cache hit, serving a stale copy.

So `api` builds only from its own `src/` and reads `frontend`'s bundle at run time,
in the uncached `serve` task (`services/api/src/serve.sh`):

```sh
#!/bin/sh
# Stand-in for the API process: serves the frontend bundle as it is on disk.
# Read at run time, never copied at build time.
set -e
echo "api serving:"
cat ../../frontend/packages/site/dist/bundle.js
```

When a downstream workspace genuinely needs a copy of an upstream artifact at
build time, that artifact belongs in the downstream workspace's own `inputs`, or
the copy has to happen somewhere Lattice's cache can see it. Otherwise the two
caches disagree about what's current and the stale one wins without a warning.

## When to wrap, when to flatten

Wrap the subtree as one workspace when:

- it has its own task runner, lockfile, and internal dependency graph, and the
  rest of the repo does not need to see or schedule that graph
- its packages are only ever built or tested together, never targeted individually
  from outside the subtree
- adopting it incrementally matters more than exposing its internals — see
  [Adopting Lattice](/lattice/docs/adopting-lattice)

Flatten it into individual workspaces when another workspace needs to depend on
one specific inner package, when you want Lattice's cache to narrow a rebuild to
the single inner package that changed, or when the inner runner is the only thing
standing between you and running one package's tests directly. Flattening trades
the one-line handoff for per-package caching and dependency edges at Lattice's own
resolution.
