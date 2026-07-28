---
title: Getting started
description: Install Lattice and run your first build.
group: Guides
order: 1
---

# Getting started

## Install

```sh
cargo install lattice
```

Verify the install:

```sh
lattice --version
```

## Point it at your repo

From the root of a monorepo, create a `lattice.json` that declares your workspaces
and tasks:

```json
{
  "workspaces": [
    { "name": "api", "path": "apps/api", "engines": { "go": ">=1.22" } },
    { "name": "web", "path": "apps/web", "engines": { "node": ">=20" } }
  ],
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] },
    "test": { "dependsOn": ["build"] }
  }
}
```

## Run a task

```sh
lattice run build
```

Lattice builds the projects in the right order, runs independent ones at the same
time, and caches each result. Run it again and the parts that didn't change come
back from cache instead of rebuilding, so you get your terminal back sooner.
