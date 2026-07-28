---
title: Getting started
description: Install Lattice and run your first build.
group: Guides
order: 1
---

# Getting started

## Install

Lattice runs on macOS and Linux. On Windows, use it inside WSL2.

Lattice is pre-release. Install it from source, which needs Rust 1.86 or newer:

```sh
cargo install --git https://github.com/latticeandcompany/lattice lattice
```

Or clone and build:

```sh
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build --release
```

Verify the install:

```sh
lattice version
```

A `curl | sh` installer that places a target-matched binary in `./.lattice/bin/`
ships with the first tagged release. Once it exists, a repo that already has a
`lattice.json` will get the exact version pinned by `latticeVersion`, so everyone on
the project runs the same build, and `rm -rf .lattice` will remove it.

## Describe your repo

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

Lattice builds the projects in dependency order and runs independent ones at the
same time. Each result is recorded; see [Caching](/lattice/docs/caching) for what a
second run skips.
