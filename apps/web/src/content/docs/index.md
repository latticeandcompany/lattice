---
title: Introduction
description: What Lattice is, the shape of a config, and where to go next.
group: Overview
order: 1
---

# Introduction

**A high-performance, local toolchain for managing monorepos.**

Lattice is one CLI that does two things, usable together or apart:

- **A task runner for monorepos in any language.** Declare workspaces and a task
  graph once, in a root `lattice.json`. Lattice runs tasks across workspaces in
  dependency order, in parallel, and caches every result by content so unchanged
  work is skipped.
- **A toolchain manager.** It pins and provisions versioned tools — compilers,
  package managers, linters — per workspace or per task, without touching your
  global environment.

You can use the task runner without ever declaring an `engine`, and you can use
the toolchain manager to pin a version even in a workspace with no tasks of its
own.

## A `lattice.json`

Everything Lattice knows about your repo comes from one file at the root. Here
is an excerpt from the one this project uses to build itself, cut down to three
workspaces and three tasks.

```json
{
  "$schema": ".lattice/schema.json",
  "workspaces": [
    { "name": "lattice-config", "path": "crates/lattice-config" },
    {
      "name": "lattice-workspace",
      "path": "crates/lattice-workspace",
      "dependsOn": ["lattice-config"]
    },
    { "name": "web", "path": "apps/web", "engines": { "node": ">=26" } }
  ],
  "engines": {
    "cargo": ">=1.86.0"
  },
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "Cargo.toml", "package.json"],
      "outputs": ["dist/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["src/**/*", "tests/**/*"]
    },
    "dev": {
      "persistent": true,
      "cache": false
    }
  }
}
```

A few things explain most of what follows. Each `workspaces` entry is a
directory, declared by a literal `path` (never a glob); `dependsOn` orders
workspaces relative to each other. `engines` are version constraints on tools
such as `cargo` and `node`; Lattice checks or provisions them per workspace.
`tasks` is the pipeline: `build`, `test`, and `dev` are names you choose, and
`^build` means "the `build` task of each dependency" while a bare `test` means
"the `test` task in this same workspace." `dev` is marked `persistent` and
uncached because it is a dev server that runs until you stop it, not a task
with a result to reuse.

Each workspace still runs its own real command underneath: `cargo test`,
`npm run build`, whatever that project already uses. Lattice reads that from
the workspace, or you set it explicitly with `scripts`.

## What `lattice run build` does

1. Reads `lattice.json`, resolves the workspaces, and expands `build` across
   every one that defines it into a dependency graph.
2. Walks that graph in order, running independent workspaces in parallel.
3. Before running each task, computes a hash over its command, its `inputs`,
   its resolved environment, and any lockfiles. It skips the task if that exact
   hash already has a cached result.
4. Runs the ones that miss, using whichever tool version the task's `engines`
   resolved to, then stores the result under that hash for next time.

Run it again with nothing changed and every task comes back from cache. Change
one file and everything that depends on it runs again; everything that doesn't
depend on it stays cached.

## Where to go next

- New to Lattice: [Installation](/lattice/docs/installation) and then
  [Getting started](/lattice/docs/getting-started) walk through `lattice init`
  to a first cached run.
- Adding it to a real repo: [Adopting Lattice](/lattice/docs/adopting-lattice)
  and [A multi-language monorepo](/lattice/docs/multi-language-monorepo).
- How tasks are ordered and cached:
  [Task graph](/lattice/docs/task-graph) and [Caching](/lattice/docs/caching).
- How tools are detected and pinned:
  [Driver detection](/lattice/docs/drivers) and
  [Engines and provisioning](/lattice/docs/engines).
- Every field, flag, and command:
  [Configuration](/lattice/docs/configuration) and
  [CLI reference](/lattice/docs/cli).
