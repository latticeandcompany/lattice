---
title: Getting started
description: From an empty repo to a task that hits the cache on its second run.
group: Overview
order: 3
---

# Getting started

This walks one repo from having no Lattice config to a task that runs once and
comes back from cache on the next run. It assumes `lattice` is already on your
`PATH` — see [Installation](/lattice/docs/installation) if it isn't.

## Scaffold a config

In the root of your repo, run:

```sh
lattice init
```

On a terminal, `init` opens a short interactive wizard: it asks whether you
need a build tool, a toolchain manager, or both, then walks you through adding
workspaces (name, path, whether to auto-detect the engine and tasks) and, for
the toolchain half, which well-known engines to pin and what version
constraint to use. Answer `n` to auto-detection for a workspace and it asks you
to pick an engine and constraint for that workspace directly. Piped output or a
non-interactive shell skips all of that.

To skip the prompts on a terminal too, pass `-y`:

```sh
lattice init -y
```

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

next: lattice run build
```

Three things landed:

- **`lattice.json`** — the config, with zero workspaces and one starter task.
- **`.lattice/schema.json`** — a committed JSON Schema so your editor can
  validate `lattice.json` as you type. It's written once and left alone after
  that; delete it and the next command rewrites it.
- **`.gitignore`** — three lines appended (existing content is untouched):
  `.lattice/cache/`, `.lattice/toolchains/`, `.lattice/bin/`. Everything under
  `.lattice/` other than `schema.json` is a per-machine artifact.

Running `init` again refuses to touch an existing `lattice.json` unless you add
`--force`.

## Read what it wrote

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

- **`$schema`** points at the committed schema file, for editor validation.
- **`latticeVersion`** pins the config to the Lattice release that scaffolded
  it. See [Upgrading](/lattice/docs/upgrading) for what happens when the
  installed binary and this field disagree.
- **`workspaces`** is empty. A workspace is a project directory declared by a
  literal path — no globs — and it's the unit Lattice runs and caches tasks
  in. See [Workspaces](/lattice/docs/workspaces).
- **`tasks.build`** is a starter task definition: `dependsOn: ["^build"]` means
  "run this workspace's dependencies' `build` first"; `outputs: ["dist/**"]`
  tells the cache which files to capture. There's no workspace to run it in
  yet, so it does nothing until you add one.

Confirm that by running it as-is:

```sh
lattice run build
```

```text
lattice: no workspaces declared. Add them to the `workspaces` array in
lattice.json to run `build`.
```

Exit code `0` — an empty repo isn't a failure, just nothing to do yet.

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

`auto: false` opts this workspace out of driver detection: instead of Lattice
reading a lockfile or manifest to infer a command, `scripts.build` is the exact
command that runs. This is the fastest way to see a real run without a
language toolchain in the picture — see
[Driver detection](/lattice/docs/drivers) for how `auto` (the default) infers
commands from evidence already in the directory instead. The task's `inputs`
list is new too: it's what the cache hashes to decide whether `app`'s `build`
needs to run again.

## Provision and install

```sh
lattice setup
```

```text
❖ lattice  setup
❖ setup complete
```

`setup` provisions any pinned toolchains this repo's `engines` declare (none,
in this example) and installs each workspace's native dependencies before you
run anything. It's safe to run again — a workspace whose lockfile hasn't
changed is skipped unless you pass `--force`. See
[Engines and provisioning](/lattice/docs/engines) for what happens once an
engine constraint is declared.

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
line-by-line log; on a terminal without it you get a live interactive display
instead, both driven from the same events — see
[Output and logging](/lattice/docs/output-modes).

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

Same hash as the cold run, so Lattice restores `app/dist` from
`.lattice/cache/` instead of re-running `scripts.build`. Delete `app/dist` and
run again — it comes back from the same cache entry without rebuilding.
Edit `app/src/index.txt` and the hash changes, so the next run is a miss
again. That hash is computed over the task's command, its `inputs`, its
resolved environment, and more; see [Caching](/lattice/docs/caching) for
exactly what goes in and what makes a cached result valid on restore.

## Next

- [Workspaces](/lattice/docs/workspaces) — what a workspace is and how
  discovery, `path`, and `dependsOn` work.
- [Task graph](/lattice/docs/task-graph) — how tasks expand across
  workspaces, `^task` vs. `task`, and parallelism.
- [Caching](/lattice/docs/caching) — the full model behind the cache hit you
  just saw.
