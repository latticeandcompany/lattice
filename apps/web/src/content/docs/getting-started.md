---
title: Getting started
description: Install Lattice and run your first build.
group: Guides
order: 1
---

# Getting started

## Install

Lattice runs on macOS and Linux. On Windows, use it inside WSL2.

Run this from the root of the repo you want to use it in:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

It downloads a binary for your platform, checks it against the published checksum,
and puts it in `./.lattice/bin/`. Nothing goes to a global path, and there is no
`PATH` to edit:

```sh
./.lattice/bin/lattice version
```

To remove it, delete the directory:

```sh
rm -rf .lattice
```

### Versions are per repo

If the directory already has a `lattice.json`, the installer reads
`latticeVersion` from it and installs exactly that version. It is read by the
installer rather than by Lattice because it has to be known before there is a
binary to read it.

From then on, the pin is what runs. A binary in `.lattice/bin` that is not the
pinned version installs the pinned one and hands the command over to it, so
checking out a branch that pins a different version is enough to run that
version — with no re-download once it is on disk.

To move a repo to a different version:

```sh
./.lattice/bin/lattice upgrade 0.2.0   # or: upgrade latest
```

That installs the version, points `.lattice/bin/lattice` at it, and writes it to
`latticeVersion`. Commit the change and everyone else moves the next time they
run a command.

### Building from source

Needs Rust 1.86 or newer:

```sh
cargo install --git https://github.com/latticeandcompany/lattice lattice
```

A binary you built yourself is left alone: in a repo that pins another version it
says so once, and runs anyway.

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
