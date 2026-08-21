---
title: Adopting Lattice
description: Adding Lattice to a repo that already builds, one workspace at a time.
group: Guides
order: 1
---

# Adopting Lattice

Adding Lattice to a repo that already builds takes three steps, in this order.
First get `lattice run` calling the exact commands you already run, one
workspace at a time. Then declare what to cache. Pin toolchains last, and only
where you need the version guarantee.

## Declare one workspace

A workspace needs a name and a directory. Add one to `lattice.json`, with one
task and no configuration at all:

```json
{
  "$schema": ".lattice/schema.json",
  "workspaces": [
    { "name": "web", "path": "apps/web" }
  ],
  "tasks": {
    "build": {}
  }
}
```

With `auto` left at its default of
`true`, Lattice reads the workspace's own lockfile or manifest to work out which
tool runs `build`. For a directory with a `package.json` and a
`package-lock.json`, that resolves to `npm run build`, the command you'd already
type by hand. A `lattice.json` with one workspace and one task is
complete and valid; you don't have to enumerate the rest first.

## Check the resolved command

`--dry-run` prints every command Lattice resolved without running any of them:

```sh
lattice run build --dry-run
```

Against this repo's own root `lattice.json`, that prints:

```text
❖ lattice  dry run · build
  → web:build  npm run build
  → lattice-output:build  cargo build
  → lattice-config:build  cargo build
  → lattice-cache:build  cargo build
  → lattice-workspace:build  cargo build
  → dagger:build  cargo build
  → lattice-runner:build  cargo build
  → lattice:build  cargo build
```

Each line is `workspace:task` and the shell command that workspace would run.
Read it against what you already run in that directory. If it matches,
`lattice run build` does the same thing through Lattice. If it doesn't, sort out
the driver (see below) before adding anything else.

## Bring workspaces in one at a time

Add a second workspace entry when you're ready for it, and use `-f`/`--filter`
to scope a run to the one you're migrating:

```sh
lattice run build --filter web
```

`--filter` matches workspaces whose name contains the pattern, and runs whatever
they depend on first. With no match it prints that nothing matched and exits
cleanly rather than erroring. Drop the filter once you trust the whole set.

Declaring a workspace doesn't require removing its `package.json` scripts,
`Makefile`, or CI step. Those keep working, called directly or through Lattice,
until you point CI at `lattice run` for that workspace too.

## When detection halts

A repo mid-migration between package managers, with a stale `package-lock.json`
and a live `pnpm-lock.yaml` both checked in, has no unambiguous answer:

```text
Error: workspace 'pkg' has an ambiguous or undeclared task driver.
Candidate tools seen: npm, pnpm
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "npm": ">=0.0.0" }
```

`auto` halts here rather than picking one of the two. The same halt happens when
a workspace has only a bare `package.json` and no lockfile, because a generic
ecosystem marker is not a strong enough signal.

There are two ways out. Declaring `engines` on the workspace names the tool, and
a declaration wins over any lockfile, so `"engines": { "pnpm": ">=8.0.0" }`
settles it and `pnpm` drives every task the usual way. That fits an ambiguity
that is just noise, like a lockfile you meant to delete. Setting `"auto": false`
and writing `scripts` fits the case where there is no right answer to infer:
the real build step is a wrapper script, or you'd rather state the command.

See [Driver detection](/lattice/docs/drivers) for the evidence ladder this halt
comes from, and what "candidate" and "role" mean there.

## Declaring `scripts`

Auto-detection assumes a plain invocation of the tool it finds: `npm run build`,
`cargo test`, `pnpm run lint`. Declare `scripts` when the workspace doesn't work
that way:

- The real command is a multi-step or wrapped script rather than a bare tool
  invocation: a `sh -c '...'` chain, or a flag set you always pass.
- The workspace is itself a monorepo with its own task runner underneath. See
  [Nested repos](/lattice/docs/nested-repos).
- The workspace's tool isn't one of the built-in drivers.

A `scripts` entry takes precedence over an inferred command for that task,
whether or not `auto` is `true`, so you can override one task and leave the rest
of the workspace on auto-detection. A workspace that sets `"auto": false` and
declares `scripts` for every task it runs composes into the graph exactly like
an auto-detected one.

## `inputs` and `outputs`

Tasks are cached by default. Until a task declares `inputs`, though, its cache
key covers only its command, its resolved environment, the toolchain, and any
lockfile present, but no source files. Such a task hits on every rerun with an
unchanged command, including the rerun right after you edit the source it builds
from, and hands back the stale result.

So `inputs` is the next thing to add, once `--dry-run` confirms the command:

```json
{
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "public/**/*", "Cargo.toml", "package.json"],
      "outputs": ["dist/**"]
    }
  }
}
```

`inputs` globs are resolved per workspace and their contents are hashed, so an
edit anywhere they match invalidates that workspace's key. `outputs` is what
gets archived on a miss and restored on a hit. A task like `test` or `lint` that
produces no artifact worth reusing can skip `outputs` and still cache correctly;
only `build`-shaped tasks need it. Setting `"cache": false` on a task keeps it
in the graph without caching it, which is the option to reach for when you
aren't ready to declare either field.

See [Caching](/lattice/docs/caching) for the rest of what the key is built from,
what counts as a hit, and how eviction works.

## Pinning toolchains

Nothing above declares an `engine`, so each detected driver is whatever tool is
already on `PATH`. Declare `engines` when you need a version constraint enforced
across every machine and CI runner that touches the workspace, or a tool
provisioned into `.lattice/toolchains` rather than installed globally. A
constraint at the root applies to every workspace; one on a workspace overrides
the root per key. Several workspaces do not need one each.

## This repo's `lattice.json`

Following that order on this project produces eight workspaces — seven Rust
crates plus the `web` app — each declared with a literal `path`:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    { "name": "lattice-config", "path": "crates/lattice-config" },
    { "name": "lattice-output", "path": "crates/lattice-output" },
    {
      "name": "lattice-workspace",
      "path": "crates/lattice-workspace",
      "dependsOn": ["lattice-config"]
    },
    {
      "name": "lattice-cache",
      "path": "crates/lattice-cache",
      "dependsOn": ["lattice-config"]
    },
    {
      "name": "dagger",
      "path": "crates/dagger",
      "dependsOn": ["lattice-config", "lattice-workspace"]
    },
    {
      "name": "lattice-runner",
      "path": "crates/lattice-runner",
      "dependsOn": [
        "dagger",
        "lattice-cache",
        "lattice-output",
        "lattice-config",
        "lattice-workspace"
      ]
    },
    {
      "name": "lattice",
      "path": "crates/lattice",
      "dependsOn": [
        "lattice-runner",
        "dagger",
        "lattice-cache",
        "lattice-output",
        "lattice-config",
        "lattice-workspace"
      ]
    },
    { "name": "web", "path": "apps/web", "engines": { "node": ">=26" } }
  ],
  "engines": { "cargo": ">=1.86.0" },
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": [
        "src/**/*",
        "public/**/*",
        "Cargo.toml",
        "package.json",
        "package-lock.json",
        "astro.config.mjs",
        "tsconfig.json"
      ],
      "outputs": ["dist/**"]
    },
    "dev": { "persistent": true, "cache": false },
    "test": { "dependsOn": ["build"], "inputs": ["src/**/*", "tests/**/*"] },
    "lint": { "inputs": ["src/**/*"] },
    "check": { "inputs": ["src/**/*", "Cargo.toml"] },
    "clean": {}
  }
}
```

Every `dependsOn` mirrors a real crate dependency in `Cargo.toml`. `web` is the
only workspace with its own `engines` entry: the root `cargo` constraint covers
every crate but says nothing about `node`. The `tasks` names are the ones each
workspace already had scripts or cargo subcommands for. `apps/web/package.json`
has no `lint` script, so `lattice run lint` creates no node for `web` and runs
the seven crates only.

`clean` is the one task with no `inputs` or `outputs`. It is cached like
anything else, by command, env, toolchain, and lockfile alone, so a second
`lattice run clean` with nothing changed is a hit and skips the delete. That is
the gap described above; it's harmless here because a skipped delete leaves
nothing stale to hand back.

## Where to go next

- [Workspaces](/lattice/docs/workspaces) — `path`, `auto`, `dependsOn`, and
  `scripts` in full.
- [Caching](/lattice/docs/caching) — what's hashed, what counts as a hit, and
  how pruning works.
- [Driver detection](/lattice/docs/drivers) — the evidence ladder, roles, and
  the composition/conflict rule behind an ambiguity halt.
- [Nested repos](/lattice/docs/nested-repos) — wrapping a workspace that has
  its own task runner underneath it.
- [Troubleshooting](/lattice/docs/troubleshooting) — symptom-to-fix for these
  failure modes.
