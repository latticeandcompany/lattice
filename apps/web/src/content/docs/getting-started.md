---
title: Getting started
description: From a monorepo with no Lattice config to a build that comes back from cache on its second run.
group: Overview
order: 3
---

# Getting started

We are going to take a small npm monorepo from no Lattice config to a build that
runs once, in dependency order, and comes back from cache on the next run.

You need `lattice` on your `PATH`. See
[Installation](/lattice/docs/installation) if you do not have it yet.

## The repo

Two packages. `app` depends on `greet`, and each has a `build` script:

```text
package.json            { "private": true, "workspaces": ["packages/*"] }
package-lock.json
packages/greet/package.json
packages/greet/src/index.js
packages/app/package.json
packages/app/src/main.js
```

```json
{
  "name": "app",
  "dependencies": { "greet": "*" },
  "scripts": {
    "build": "mkdir -p dist && cp src/main.js dist/main.js",
    "test": "node src/main.js"
  }
}
```

`greet`'s `package.json` has a `build` script and nothing else. If you are
following along in your own repo, any npm, pnpm, yarn, Cargo, Go or uv
workspace works the same way.

## Initialize

From the repo root:

```sh
lattice init
```

`init` reads the repo before it writes anything, then shows what it found:

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

2 workspaces:
  app    npm      build test
  greet  npm      build

next: lattice run build
```

Open `lattice.json`:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "2.0.0"
}
```

That is the whole config. The workspaces came from `package.json`'s
`workspaces`, the tool from `package-lock.json`, the tasks from each package's
`scripts`, and the order from `app`'s dependency on `greet`. `lattice.json` only
holds what the repo does not already say, and this repo already says
everything. `$schema` points at the committed schema your editor validates
against; `latticeVersion` pins the release this repo runs (see
[Upgrading](/lattice/docs/upgrading)).

`init` also ignores Lattice's per-machine files in `.gitignore`:
`.lattice/cache/`, `.lattice/toolchains/`, `.lattice/bin/`, `.lattice/setup/`,
and `.lattice-setup-marker`. Commit `lattice.json` and `.lattice/schema.json`.

In a repo with projects no workspace file names, say a Go service beside the
packages, `init` offers to add them under `workspaces`, and it offers any tool
version the repo already pins in `.nvmrc` or `.tool-versions` as an `engines`
entry. On a terminal you confirm each list; `-y` takes them as offered.

## Check what will run

```sh
lattice run build --dry-run
```

```text
❖ lattice  dry run · build
  → greet:build  npm run build
  → app:build  npm run build
```

`greet` comes first because `app` depends on it. `build` waits on the builds of
its dependencies without being told to. See [Task
graph](/lattice/docs/task-graph).

## Install dependencies

```sh
lattice setup
```

`setup` checks or installs any tool versions pinned under `engines`, then runs
each workspace's installer from the nearest directory holding its own tool's
lockfile, once per lockfile: here, one `npm install` at the root. On a fresh
clone, run it before the first build.

## Run it

```sh
lattice run build
```

On a terminal, the run draws a live display: each running task with the last
lines it printed, the tasks still waiting and what they wait on, and progress.
Finished tasks settle into a line each:

```text
❖ lattice  build  · 2 workspaces

  ✓ greet  0.12s
  ✓ app    0.12s

❖  2 tasks  0.27s
```

`packages/greet/dist` and `packages/app/dist` now exist. An npm workspace's
`build` caches `dist/**` unless you say otherwise.

## Run it again

Nothing changed, so nothing runs:

```text
❖ lattice  build  · 2 workspaces

  ● greet  cached · 0.12s saved
  ● app    cached · 0.12s saved

❖  2 tasks · 2 cached  0.00s · 0.23s saved

❖❖❖ FULL POWER
```

Each package's `dist` was restored from `.lattice/cache/`. Delete
`packages/app/dist` and run again: it comes back from the same entry. Edit
`packages/greet/src/index.js` and run again: `greet` builds, and so does `app`,
because a dependency's change reaches every package that depends on it. See
[Caching](/lattice/docs/caching).

## Run from inside a package

```sh
cd packages/greet
lattice run build
```

A run started inside a workspace covers that workspace and what it depends on,
so this builds `greet` alone. From `packages/app` it would build `greet` and
then `app`.

A run from inside a workspace that has none of the tasks named fails, rather
than running nothing and exiting 0. `greet` has no `test` script:

```text
$ lattice run test
error: greet, the workspace you are in, has no 'test' task. Run it from the repo root to run it everywhere, or pick workspaces with --filter
```

## Pass arguments through

From the repo root, anything after `--` goes to the task you named:

```sh
lattice run test -- --trace-warnings
```

runs `npm run test -- --trace-warnings` in `app`, the only package with a
`test` script.

## Next

- [Adopting Lattice](/lattice/docs/adopting-lattice) for a larger repo, with
  more than one language.
- [Workspaces](/lattice/docs/workspaces) for how Lattice finds and names
  workspaces.
- [Driver detection](/lattice/docs/drivers) for which tasks each tool can run.
- [Output and logging](/lattice/docs/output-modes) for the plain stream CI gets.
