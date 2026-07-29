---
title: Adopting Lattice
description: Adding Lattice to a repo that already builds, one workspace at a time.
group: Guides
order: 1
---

# Adopting Lattice

You already have a repo that builds, tests, and ships. Nothing about adding
Lattice requires touching any of that first. The order that works is: get
`lattice run` calling the exact commands you already run, one workspace at a
time; only then teach it what to cache; pin toolchains last, and only where
you actually need the guarantee. Skipping ahead — declaring `inputs` before
the command is right, or pinning an `engine` before you have a second
workspace to share it with — is what makes an adoption feel like a rewrite.

## Start with the workspaces you have, not the tasks you want

A workspace is a directory and a name — nothing else is required to start.
Add one to `lattice.json`, and one task with no configuration at all:

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

`path` is a literal directory, never a glob. With `auto` left at its default
of `true`, Lattice reads the workspace's own lockfile or manifest to work out
which tool runs `build` — for a directory with a `package.json` and a
`package-lock.json`, that resolves to `npm run build`, the same command you'd
already type by hand. You do not need to enumerate every workspace before
this is useful; a `lattice.json` with one workspace and one task is a
complete, valid file.

## Confirm the command before you trust it

Every task command Lattice resolves is inspectable before anything runs:

```sh
lattice run build --dry-run
```

Run against this repo's own root `lattice.json`, that prints:

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

Each line is `workspace:task` and the exact shell command that workspace
would run — nothing executes. Read it against what you already run in that
directory. If it matches, `lattice run build` (without `--dry-run`) does the
same thing you were already doing, just through Lattice. If it doesn't match,
you have a driver problem to sort out (see below) before you add anything
else.

## Bring workspaces in one at a time

You don't have to convert a repo in one pass. Add a second workspace entry
when you're ready for it, and use `-f`/`--filter` to scope a run to just the
one you're currently migrating, without disturbing anyone still using the
old command directly in every other directory:

```sh
lattice run build --filter web
```

`--filter` matches workspaces whose name contains the given pattern; run
against no match, it prints that nothing matched and exits cleanly rather
than erroring. Nothing about adding a workspace to `lattice.json` requires
removing its existing `package.json` scripts, `Makefile`, or CI step — a
workspace's own scripts keep working exactly as before, called directly or
through Lattice, until you're ready to point CI at `lattice run` for that
workspace too. Drop the filter once you trust the whole set.

## When detection halts on a workspace

Not every directory has one unambiguous answer. A repo mid-migration between
package managers, with both a stale `package-lock.json` and a live
`pnpm-lock.yaml` still checked in, is a real and common case:

```text
Error: workspace 'pkg' has an ambiguous or undeclared task driver.
Candidate tools seen: npm, pnpm
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "npm": ">=0.0.0" }
```

This is `auto` refusing to guess rather than picking one silently — the same
happens if a workspace has only a bare `package.json` with no lockfile at
all, since a generic ecosystem marker isn't a strong enough signal on its
own. You have two ways out, and the choice is which one matches reality:

- **Declare `engines` naming the tool.** A declaration always wins over any
  lockfile, so `"engines": { "pnpm": ">=8.0.0" }` on that workspace settles it
  for good — use this when the ambiguity is genuinely just noise (a lockfile
  you meant to delete) and the winning tool should drive every task the usual
  way.
- **Set `"auto": false` and write `scripts`.** Use this when there isn't
  really one right answer to infer — the workspace's real build step is a
  wrapper script, or you'd rather state the command than have Lattice guess
  at it correctly.

See [Driver detection](/lattice/docs/drivers) for the full evidence ladder
this halt comes from, and what "candidate" and "role" mean there.

## When to reach for `scripts` instead of fighting detection

Auto-detection assumes a plain invocation of the tool it finds — `npm run
build`, `cargo test`, `pnpm run lint`. Plenty of real workspaces don't work
that way, and declaring `scripts` there isn't a fallback, it's the correct
answer:

- The real command is a multi-step or wrapped script, not a bare tool
  invocation (a `sh -c '...'` chain, a custom flag set you always pass).
- The workspace is itself a monorepo with its own task runner underneath —
  see [Nested repos](/lattice/docs/nested-repos) for that shape end to end.
- The workspace's tool isn't one of the built-in drivers at all.

`scripts` entries always take precedence over an inferred command, for any
task, whether or not `auto` is `true` — so you can override just the one task
that needs it and leave every other task in that workspace on auto-detection.
A workspace can also set `"auto": false` and declare `scripts` for every task
it runs, opting out of inference entirely; that workspace still composes into
the rest of the graph exactly like an auto-detected one.

## Add `inputs` and `outputs` before you trust the cache

A task is cached by default — nothing you've done so far turns that off. But
until a task declares `inputs`, its cache key does not include your source
files at all: only its command, its resolved environment, the toolchain, and
any lockfile present. That means a task with no `inputs` is a cache hit on
every rerun with an unchanged command, including a rerun right after you
edited the source it builds from. That isn't a miss that should have been a
hit; it's a stale result served with confidence.

This is why `inputs` is the next thing to add, once `--dry-run` confirms the
command, not before:

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

`inputs` globs are resolved per workspace and their contents are hashed, so
an edit anywhere they match invalidates that workspace's key. `outputs` is
separate and equally worth declaring deliberately: it's what actually gets
archived on a miss and restored on a hit. A task like `test` or `lint` that
produces no artifact worth reusing can skip `outputs` entirely and still
cache correctly — only `build`-shaped tasks need it. If you're not ready to
reason about either yet, `"cache": false` on that one task keeps it in the
graph without caching it at all, which is a safer default than an
undeclared, silently-cached task.

See [Caching](/lattice/docs/caching) for the rest of what the key is built
from, what counts as a hit, and how eviction works.

## Pin toolchains once the graph is real

Nothing above required declaring an `engine` anywhere — the driver Lattice
detected is whatever tool is already on `PATH`. Reach for `engines` only once
you need more than that: a version constraint enforced across every machine
and CI runner that touches the workspace, or a tool provisioned into
`.lattice/toolchains` because it isn't something you want installed
globally. A constraint can live at the root, where it applies to every
workspace, or on one workspace, where it overrides the root per key — you
don't need one per workspace just because you have several.

## A mid-size example: this repo's own `lattice.json`

This project's own root `lattice.json` is what following this order actually
produces — eight workspaces (seven Rust crates plus the `web` app), each
declared with a literal `path`:

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

Every `dependsOn` here mirrors a real crate dependency in `Cargo.toml` —
nothing was invented for the graph. `web` is the only workspace that needed
its own `engines` entry, because it's the one workspace whose toolchain
(`node`) the root `cargo` constraint says nothing about; every crate is
covered by that single root line. `tasks` are the same five or six names
each workspace already had scripts or cargo subcommands for — `test` and
`lint` simply don't apply to `web`, which has neither, and no node is
created for them there. `clean` is the one task left with no `inputs` or
`outputs` at all: it's still cached the same as anything else (by command,
env, toolchain, and lockfile alone), so a second `lattice run clean` with
nothing else changed comes back as a hit and skips re-running it. That's the
same gap called out above — harmless here only because a skipped delete
leaves nothing stale behind for a task to hand back.

## Where to go next

- [Workspaces](/lattice/docs/workspaces) — the full model behind `path`,
  `auto`, `dependsOn`, and `scripts`.
- [Caching](/lattice/docs/caching) — exactly what's hashed, what counts as a
  hit, and how pruning works.
- [Driver detection](/lattice/docs/drivers) — the evidence ladder, roles, and
  the composition/conflict rule behind every ambiguity halt.
- [Nested repos](/lattice/docs/nested-repos) — wrapping a workspace that
  already has its own task runner underneath it.
- [Troubleshooting](/lattice/docs/troubleshooting) — symptom-to-fix for the
  failure modes this guide only summarizes.
