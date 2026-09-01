---
title: Getting started
description: From an empty repo to a task that comes back from cache on its second run.
group: Overview
order: 3
---

# Getting started

We are going to take one repo from no config to a task that runs once and comes
back from cache on the next run.

You need `lattice` on your `PATH`. See
[Installation](/lattice/docs/installation) if you do not have it yet.

## Scaffold a config

From the root of your repo, run:

```sh
lattice init
```

`init` reads the repo before it writes anything. On a terminal you get two
checklists to confirm: every directory holding a manifest it recognizes, and
every tool version the repo already pins in a file like `.nvmrc`,
`.tool-versions`, or `rust-toolchain.toml`. Uncheck whatever is wrong.

```text
found 2 workspaces
> [x] apps/web        pnpm       package.json
  [x] services/api    cargo      Cargo.toml

found 1 pinned tool version
> [x] node         22.11.0        .nvmrc
```

To take the scan's proposal without confirming, pass `-y`. A pipe or a
non-interactive shell does the same on its own:

```sh
lattice init -y
```

The rest of this guide builds a repo up from nothing, so run it somewhere with
no manifests to find. You should see three files reported and a hint about what
to do next:

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

next: declare a workspace in lattice.json
```

`lattice.json` is the config. `.lattice/schema.json` is a committed JSON Schema
your editor validates against as you type. In `.gitignore`, five lines are
appended and the existing content is left alone: `.lattice/cache/`,
`.lattice/toolchains/`, `.lattice/bin/`, `.lattice/setup/`, and
`.lattice-setup-marker`.

To scaffold over a `lattice.json` that already exists, add `--force`. Without
it, `init` refuses.

## Read what it wrote

Open `lattice.json`:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0",
  "tasks": {
    "build": {
      "dependsOn": [
        "^build"
      ],
      "outputs": [
        "dist/**"
      ]
    }
  },
  "workspaces": []
}
```

Four things are in there. `$schema` points at the committed schema file.
`latticeVersion` pins the release this repo runs on (see
[Upgrading](/lattice/docs/upgrading)). `tasks.build` declares one task, where
`dependsOn: ["^build"]` means "build my dependencies first" and `outputs` tells
the cache which files to capture (see [Task
graph](/lattice/docs/task-graph)). `workspaces` is empty, and a workspace is a
directory with its own manifest that Lattice runs tasks in (see
[Workspaces](/lattice/docs/workspaces)).

Nothing is declared to run yet. Confirm that:

```sh
lattice run build
```

```text
lattice: no workspaces declared. Add one to the `workspaces` array in lattice.json, then run `build`.
```

Check the exit code with `echo $?`. It is `0`. An empty repo is nothing to do,
not a failure.

## Add a workspace

Make a directory with something to build:

```sh
mkdir -p app/src
echo hello > app/src/index.txt
```

Now declare it. Replace `lattice.json` with this:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0",
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

Two fields are new. `auto: false` opts this workspace out of driver detection,
so `scripts.build` is exactly what runs (see [Driver
detection](/lattice/docs/drivers) for what the default, `auto: true`, infers
instead). The task's `inputs` names the files whose contents decide whether
`build` needs to run again.

Check that Lattice resolved the command you wrote:

```sh
lattice run build --dry-run
```

```text
❖ lattice  dry run · build
  → app:build  mkdir -p dist && cp src/index.txt dist/index.txt
```

## Provision and install

```sh
lattice setup
```

```text
❖ lattice  setup
❖ setup complete
```

`setup` provisions the toolchains this repo pins under `engines` and installs
each workspace's dependencies. This repo pins nothing and has no dependencies,
so it finishes immediately. See [Pinning tool
versions](/lattice/docs/pinning-tool-versions) for what it does once `engines`
has an entry.

## Run the task

```sh
lattice run build -v
```

```text
lattice: running `build` across 1 workspace
lattice: app:build: hash 92e4f1987f6770d8
lattice: app:build: cache miss (nothing cached for this task yet)
app:build: running
app:build: done (0.01s)
lattice: 1 tasks, 0 cached, 0 failed, 0.01s
```

Your hash may differ from this one. It covers the platform, the shell, and the
Lattice version as well as the task's inputs. `app/dist/index.txt` now exists.

`-v` prints this plain line-by-line log. Drop it and a terminal gets a live
display of the same run instead. That display leaves out the `hash` and `cache
miss` lines, which only `-v` prints. See [Output and
logging](/lattice/docs/output-modes).

## Run it again

Nothing under `app/src` changed, so the second run does not execute the command
at all:

```sh
lattice run build -v
```

```text
lattice: running `build` across 1 workspace
lattice: app:build: hash 92e4f1987f6770d8
app:build: cache hit [92e4f198]
lattice: 1 tasks, 1 cached, 0 failed, 0.00s, 0.01s saved
lattice: full power, nothing to run
```

Same hash as the first run, so Lattice restored `app/dist` from
`.lattice/cache/` instead of running `scripts.build` again.

Two more things to try. Delete `app/dist` and run again: it comes back from the
same entry without rebuilding. Then edit `app/src/index.txt` and run again: the
hash changes and the command runs. See [Caching](/lattice/docs/caching) for
everything that feeds that hash.

## Next

- [Adopting Lattice](/lattice/docs/adopting-lattice) to bring a repo that
  already builds into Lattice one workspace at a time.
- [Workspaces](/lattice/docs/workspaces) for `path`, `auto`, `dependsOn`, and
  `scripts` in full.
- [Task graph](/lattice/docs/task-graph) for how one task expands across
  workspaces, and what `^build` does.
