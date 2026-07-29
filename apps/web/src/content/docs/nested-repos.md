---
title: Nested repos
description: Wrapping a subtree that already has its own task runner.
group: Guides
order: 3
---

# Nested repos

Some subtrees in a monorepo are not a single project. They are a project *of*
projects, already wired together by a task runner of their own: a JS monorepo
with its own task graph, a Gradle multi-module build, a directory driven by a
`Makefile`. You do not need to flatten that subtree into individual Lattice
workspaces to bring it into the graph. Declare it as one workspace whose
`scripts` shell out to the runner it already has, and it becomes a single node
that Lattice schedules and caches like any other.

This page walks through `examples/nested-repo`, a small repo with exactly this
shape, and the commands below are real output from running it.

## The shape

```text
lattice.json          two workspaces: frontend, api
frontend/             an inner monorepo with its own task runner and lockfile
  packages/ui/         built first (inner dependency)
  packages/site/       depends on ui, emits dist/bundle.js
services/api/          a plain workspace; serves site's bundle at run time
```

`frontend` wraps a repo with its own task runner and its own inner dependency
graph (`ui` before `site`). `services/api` is an ordinary workspace that reads
`frontend`'s output. The root `lattice.json`:

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
`node_modules`, installed the ordinary way for that ecosystem
(`cd frontend && npm install`). See
[Workspaces](/lattice/docs/workspaces) for `path`, `dependsOn`, and `scripts` in
general; this page covers the one pattern where a workspace's script hands off
to a whole other build.

## `auto: false`: full manual control

Both workspaces set `"auto": false`. On an `auto` workspace, Lattice runs the
evidence ladder to detect a driver and infers commands from it; on a manual
workspace it skips detection entirely and runs only the commands you wrote in
`scripts`. For a wrapped subtree this is not optional: the inner directory may
contain a lockfile or config file Lattice would otherwise use to infer a
command for the *outer* language, and that guess would be wrong — the real
entry point is "invoke the inner runner," not "run this package's own script."
`auto: false` says exactly that: no inference, only what you declared.

A manual workspace must declare a script for any task you run directly against
it. `frontend` has no `serve` script, because there is nothing for the frontend
to serve. Running `serve` scoped to it fails to resolve; running it repo-wide
(or filtered to `api`) works, because tasks that are only pulled in as
dependencies are skipped where a workspace doesn't apply.

## `inputs` and `outputs`: still yours to declare

The inner runner already tracks its own file dependencies and has its own
cache. That does not remove the need to declare `inputs` and `outputs` in
`lattice.json` — it changes what they need to cover. Lattice hashes `frontend`
as one opaque unit; it has no visibility into `ui` or `site` individually, so
its `inputs` glob has to be broad enough to catch a change anywhere inside:

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

`**/*` is deliberately wide, because the wrapped repo owns its own internal
layout and Lattice is not going to enumerate it. The precision moves to
`ignore`, and three things have to stay out of the hash:

- `**/node_modules/**` — the installed tree; the package manager's lockfile is
  hashed instead (see [Caching](/lattice/docs/caching))
- `**/.turbo/**` — the inner runner's own cache directory, which changes on
  every run regardless of source changes and would make every build a miss
- `**/dist/**` — build output; an output glob left inside `inputs` would feed
  yesterday's build into today's cache key

`outputs` still has to name every artifact directory the wrapped repo
produces, at every level (`dist/**` for `api`, `packages/*/dist/**` for each
inner package) — Lattice restores exactly those paths on a hit, and no others.

## Two caches, not one

Wrapping a subtree does not disable its inner cache. It adds a second one
above it, and each skips a different kind of repeated work:

- **Lattice's cache** covers `frontend` as a whole. A hit restores every inner
  package's `dist/` in one step and skips invoking the inner runner at all.
- **The inner runner's cache** covers individual inner packages, and only runs
  at all on a Lattice miss — when `frontend`'s hash has changed and Lattice
  hands the whole workspace back to it. From there the inner runner still
  decides, package by package, what actually needs to rerun.

So a change inside `frontend` costs one inner rebuild the first time (Lattice
miss, inner cache narrows it to the package that changed) and zero the second
time (Lattice hit, the inner runner never runs). Neither cache makes the other
redundant: Lattice's is coarse and language-agnostic, which is why the rest of
the repo can treat `frontend` as one node; the inner one is fine-grained and
specific to that ecosystem.

## Running it

With the inner repo's dependencies installed once (`cd frontend && npm
install`), from the repo root:

```sh
$ lattice run build
frontend:build: running
frontend:build: done (0.66s)
api:build: running
api:build: done (0.01s)
lattice: 2 tasks, 0 cached, 0 failed, 0.68s
```

Run it again with nothing changed and both workspaces come back from cache
without the inner runner executing:

```sh
$ lattice run build
frontend:build: cache hit [587c2274]
api:build: cache hit [11b8de60]
lattice: 2 tasks, 2 cached, 0 failed, 0.01s
```

Edit a file anywhere under `frontend` (say, `packages/ui/src/index.js`) and
only `frontend` misses; `api` still depends on `frontend` completing, but its
own inputs haven't changed:

```sh
$ lattice run build
frontend:build: running
frontend:build: done (0.52s)
api:build: cache hit [11b8de60]
lattice: 2 tasks, 1 cached, 0 failed, 0.52s
```

`--dry-run` shows the resolved commands without running them — useful for
confirming the handoff script is right before it executes:

```sh
$ lattice run build --dry-run
❖ lattice  dry run · build
  → frontend:build  sh -c 'if [ -x node_modules/.bin/turbo ]; then node_modules/.bin/turbo run build; else echo "[frontend] turbo is not installed — run: (cd frontend && npm install)" >&2; exit 1; fi'
  → api:build  mkdir -p dist && cp src/serve.sh dist/serve.sh && chmod +x dist/serve.sh && echo 'api built'
```

`serve` is filtered to `api`, since `frontend` has nothing to serve
(`--filter` is covered in full in
[Selecting what runs](/lattice/docs/filtering)):

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

A workspace's cache key covers only its own declared `inputs`; it does not
fold in the keys of the workspaces it depends on, and an `inputs` glob cannot
reach outside its own workspace directory. So if `api:build` copied
`frontend`'s bundle into its own `dist/`, a `frontend`-only edit would rebuild
`frontend` and still hand `api` a cache hit — serving a stale copy.

That is why, in this example, `api` builds only from its own `src/` and reads
`frontend`'s bundle at run time instead, in the uncached `serve` task
(`services/api/src/serve.sh`):

```sh
#!/bin/sh
# Stand-in for the API process: serves the frontend bundle as it is on disk.
# Read at run time, never copied at build time.
set -e
echo "api serving:"
cat ../../frontend/packages/site/dist/bundle.js
```

If a downstream workspace genuinely needs a copy of an upstream build
artifact at build time, that artifact belongs in the downstream workspace's
own `inputs`, or the copy step has to happen somewhere Lattice's cache can see
it — otherwise the two caches disagree about what's current and the second one
wins silently.

## When to wrap, when to flatten

Wrap the subtree as one workspace when:

- it already has its own task runner, lockfile, and internal dependency
  graph, and that graph is not something the rest of the repo needs to see or
  schedule around
- the packages inside it are only ever built or tested together, never
  targeted individually from outside the subtree
- adopting it incrementally matters more than exposing its internals — see
  [Adopting Lattice](/lattice/docs/adopting-lattice)

Flatten it into individual workspaces instead when another workspace in the
repo needs to depend on *one specific inner package* rather than the whole
subtree, when you want Lattice's cache to narrow a rebuild down to the single
inner package that changed instead of the whole wrapped unit, or when the
inner runner would otherwise become the only thing standing between you and
running a single package's tests directly. Flattening costs you the one-line
handoff; it buys you per-package caching and dependency edges at Lattice's own
level of resolution.
