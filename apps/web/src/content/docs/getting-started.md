---
title: Getting started
description: From an empty repo to a task that hits the cache on its second run.
group: Overview
order: 3
---

# Getting started

This takes one repo from no Lattice config to a task that runs once and comes
back from cache on the next run. It assumes `lattice` is on your `PATH`; see
[Installation](/lattice/docs/installation) if it isn't.

## Scaffold a config

In the root of your repo, run:

```sh
lattice init
```

`init` reads the repo before it writes anything. It walks the tree for
directories holding a manifest it recognizes and proposes each one as a
workspace, and it reads the tool versions you already record in files like
`.nvmrc`, `.tool-versions`, and `rust-toolchain.toml` and proposes those as
engines. On a terminal you get both lists pre-checked, and uncheck whatever is
wrong:

```text
found 2 workspaces
> [x] apps/web        pnpm       package.json
  [x] services/api    cargo      Cargo.toml

found 1 pinned tool version
> [x] node         22.11.0        .nvmrc
```

If it finds nothing to propose, it asks you to declare at least one workspace or
one engine — a config with neither does nothing, so `init` will not write one.

To take the scan's proposal without confirming, pass `-y`. Piped output or a
non-interactive shell does the same thing on its own:

```sh
lattice init -y
```

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

next: lattice run build
```

`lattice.json` is the config: the workspaces the scan found and one starter
task. `.lattice/schema.json` is a committed JSON Schema your editor can validate
against as you type; delete it and the next command rewrites it. In
`.gitignore`, three lines are appended and existing content is untouched:
`.lattice/cache/`, `.lattice/toolchains/`, and `.lattice/bin/`, the per-machine
artifacts under `.lattice/`.

Running `init` again refuses to touch an existing `lattice.json` unless you add
`--force`.

## Read what it wrote

The rest of this guide builds a repo up from nothing, so assume the scan found
nothing to propose and `init` wrote the bare skeleton:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "outputs": ["dist/**"]
    }
  },
  "workspaces": []
}
```

`$schema` points at the committed schema file, for editor validation.
`latticeVersion` pins the config to the release that scaffolded it; see
[Upgrading](/lattice/docs/upgrading) for what happens when the installed binary
and this field disagree. `workspaces` is empty — a workspace is a project
directory declared by name and path, and it is the unit Lattice runs and caches
tasks in (see [Workspaces](/lattice/docs/workspaces)). In
`tasks.build`, `dependsOn: ["^build"]` means "run this workspace's dependencies'
`build` first" and `outputs: ["dist/**"]` tells the cache which files to
capture. There's no workspace to run it in yet, so it does nothing until you add
one.

Confirm that by running it as-is:

```sh
lattice run build
```

```text
lattice: no workspaces declared. Add them to the `workspaces` array in
lattice.json to run `build`.
```

Exit code `0`. An empty repo is nothing to do, not a failure.

## Add a workspace and a task

Add a directory with something to build, and declare it as a workspace:

```sh
mkdir -p app/src
echo hello > app/src/index.txt
```

Edit `lattice.json`:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    {
      "name": "app",
      "path": "app",
      "auto": false,
      "scripts": { "build": "mkdir -p dist && cp src/index.txt dist/index.txt" }
    }
  ],
  "tasks": {
    "build": {
      "inputs": ["src/**/*"],
      "outputs": ["dist/**"]
    }
  }
}
```

`auto: false` opts this workspace out of driver detection: `scripts.build` is
the exact command that runs, and no lockfile or manifest is read to infer one.
See [Driver detection](/lattice/docs/drivers) for how `auto` (the default)
infers a command from evidence already in the directory. The task's `inputs`
list is what the cache hashes to decide whether `app`'s `build` needs to run
again.

## Provision and install

```sh
lattice setup
```

```text
❖ lattice  setup
❖ setup complete
```

`setup` provisions any pinned toolchains this repo's `engines` declare (none, in
this example) and installs each workspace's native dependencies. Run it again
and a workspace whose lockfile hasn't changed is skipped, unless you pass
`--force`. See [Engines and provisioning](/lattice/docs/engines) for what
happens once an engine constraint is declared.

## Run the task

```sh
lattice run build -l
```

```text
lattice: running `build` across 1 workspace(s)
lattice: app:build: hash 8aecd62e96682197
lattice: app:build: cache miss
app:build: running
app:build: done (0.01s)
lattice: 1 tasks, 0 cached, 0 failed, 0.01s
```

`app/dist/index.txt` now exists. `-l` (`--loquacious`) prints this plain,
line-by-line log; without it, a terminal gets a live interactive display driven
from the same events. See [Output and logging](/lattice/docs/output-modes).

## Run it again

Nothing changed under `app/src`, so the second run doesn't execute the command
at all:

```sh
lattice run build -l
```

```text
lattice: running `build` across 1 workspace(s)
lattice: app:build: hash 8aecd62e96682197
app:build: cache hit [8aecd62e]
lattice: 1 tasks, 1 cached, 0 failed, 0.00s
```

Same hash as the cold run, so Lattice restores `app/dist` from `.lattice/cache/`
instead of re-running `scripts.build`. Delete `app/dist` and run again: it comes
back from the same cache entry without rebuilding. Edit `app/src/index.txt` and
the hash changes, so the next run misses. The hash covers the task's command,
its `inputs`, its resolved environment, and more; see
[Caching](/lattice/docs/caching) for exactly what goes in and what makes a
cached result valid on restore.

## Next

- [Workspaces](/lattice/docs/workspaces) — what a workspace is and how
  discovery, `path`, and `dependsOn` work.
- [Task graph](/lattice/docs/task-graph) — how tasks expand across
  workspaces, `^task` vs. `task`, and parallelism.
- [Caching](/lattice/docs/caching) — what goes into a cache key, and what
  counts as a hit.
