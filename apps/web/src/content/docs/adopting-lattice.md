---
title: Adopting Lattice
description: Add Lattice to a repo that already builds, one workspace at a time.
group: Guides
order: 1
---

# Adopting Lattice

Do this in three passes. Get `lattice run` calling the commands you already run,
one workspace at a time. Then narrow what each task caches. Pin toolchains last,
and only where you need the version guarantee.

Nothing here requires you to stop using what the repo has now. A declared
workspace keeps its `package.json` scripts, its `Makefile`, and its CI step, all
still callable directly, until you point CI at `lattice run` too.

## Declare one workspace

A workspace needs a name and a directory. Write one entry and one task:

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

That config is complete and valid. You do not have to enumerate the rest of the
repo first.

`auto` defaults to `true`, so Lattice reads the workspace's own lockfile or
manifest to work out which tool runs `build`. A directory with a `package.json`
and a `package-lock.json` resolves to `npm run build`, the command you would
already type there.

## Check the resolved command

```sh
lattice run build --dry-run
```

```text
❖ lattice  dry run · build
  → web:build  npm run build
```

Each line is `workspace:task` and the command that workspace would run. Read it
against what you run in that directory today. If it matches, `lattice run build`
does the same work through Lattice. If it does not, fix the driver before you
add anything else.

## Bring in the next workspace

Add the entry, then scope the run to the workspace you are migrating:

```sh
lattice run build --filter web
```

`--filter` matches workspaces whose name contains the pattern, and runs whatever
they depend on first. A pattern that matches nothing prints a line and exits
`0`. Drop the filter once you trust the whole set. See [Selecting what
runs](/lattice/docs/filtering).

## Fix a workspace that halts on detection

A repo part-way between package managers, with a stale `package-lock.json` and a
live `pnpm-lock.yaml` both checked in, has no unambiguous answer:

```text
Error: workspace 'pkg' has an ambiguous or undeclared driver.
Candidate drivers: npm, pnpm
Declare the driver in lattice.json, under this workspace:
  "engines": { "npm": ">=0.0.0" }
```

Lattice halts here instead of picking one. The same halt happens when a
workspace has only a bare `package.json` and no lockfile, because an ecosystem
marker on its own does not name a tool.

You have two ways out.

To name the tool, declare `engines` on the workspace. A declaration beats any
lockfile, so `"engines": { "pnpm": ">=8.0.0" }` settles it and `pnpm` drives
every task as usual. Reach for this when the ambiguity is noise, such as a
lockfile you meant to delete. Note that the suggestion in the error message is
one of the candidates, not necessarily the one you want.

To state the commands yourself, set `"auto": false` and write `scripts`. Reach
for this when there is nothing right to infer: the real build step is a wrapper
script, or the tool is not one Lattice knows.

See [Driver detection](/lattice/docs/drivers) for the evidence ladder this halt
comes from.

## Declare `scripts` when a plain invocation is wrong

Detection assumes a plain invocation of the tool it finds: `npm run build`,
`cargo test`, `pnpm run lint`. Declare `scripts` when the workspace does not
work that way:

- The real command is multi-step or wrapped, such as a `sh -c '…'` chain or a
  flag set you always pass.
- The workspace is itself a monorepo with its own task runner underneath. See
  [Nested repos](/lattice/docs/nested-repos).
- The workspace's tool is not one of the built-in drivers.

A `scripts` entry beats an inferred command for that task whether or not `auto`
is `true`, so you can override one task and leave the rest of the workspace on
detection.

## Narrow what each task hashes

Tasks are cached by default, and a task with no `inputs` hashes every file in
its workspace apart from what `.gitignore` excludes and what the task's own
`outputs` match. That is correct but coarse: a README edit invalidates a build.
Declaring `inputs` narrows it to the files that matter:

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

`inputs` globs resolve per workspace and their contents are hashed, so an edit
anywhere they match invalidates that workspace's key. `outputs` is what gets
archived on a miss and restored on a hit.

To keep a task in the graph without caching it, set `"cache": false`. That is
the option to reach for while you are still working out what a task reads and
writes.

For a file above the workspace, such as a shared `tsconfig.base.json` or a
schema directory, no `inputs` glob can name it. Put it in the root-level
`globalDependencies` instead, and put repo-wide variables in `globalEnv`:

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"]
}
```

If a workspace's `turbo.json` declared either key, `init` already wrote it and
the job here is to prune rather than to add.

See [Caching](/lattice/docs/caching) for the rest of what the key is built from.

## Pin toolchains last

Nothing above declares an engine, so each detected driver is whatever is already
on `PATH`. Declare `engines` when you need a version constraint enforced on
every machine and CI runner that touches the workspace, or a tool provisioned
into `.lattice/toolchains` rather than installed globally.

A constraint at the root applies to every workspace. One on a workspace
overrides the root key by key. Most repos need one root entry, not one per
workspace. See [Pinning tool
versions](/lattice/docs/pinning-tool-versions).

## Where to go next

- [Workspaces](/lattice/docs/workspaces) for `path`, `auto`, `dependsOn`, and
  `scripts` in full.
- [Caching](/lattice/docs/caching) for what is hashed and what counts as a hit.
- [Driver detection](/lattice/docs/drivers) for the evidence ladder and roles
  behind an ambiguity halt.
- [Nested repos](/lattice/docs/nested-repos) for wrapping a workspace that has
  its own task runner.
- [Troubleshooting](/lattice/docs/troubleshooting) for symptom-to-fix on these
  failure modes.
