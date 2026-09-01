---
title: Driver detection
description: How Lattice picks the tool that runs a workspace's tasks, and when it stops to ask.
group: Concepts
order: 4
---

# Driver detection

A task named `build` has to become a real shell command. The tool that supplies
that command is the workspace's **driver**.

The evidence that identifies a language rarely identifies a driver. A lone
`package.json` says JavaScript. It does not say whether `build` means
`pnpm run build`, `yarn build`, `npm run build`, or `bun run build`, and those
four commands can behave differently. Lattice looks for evidence that names one
tool, and halts when it finds none.

## The evidence ladder

For each workspace, Lattice gathers candidate tools from three kinds of
evidence. A tool named by more than one kind keeps the highest-ranked:

| Rung | Evidence | Examples |
| --- | --- | --- |
| 1. Declaration | A driver named in the workspace's resolved `engines` map, meaning its own entries merged over the root's | `"engines": { "pnpm": ">=8" }` |
| 2. Native file | A file the developer wrote to pin a tool | `packageManager` in `package.json`, `.tool-versions`, a `[tools]` table in `mise.toml` or `.mise.toml`, `.nvmrc`, `rust-toolchain.toml` or `rust-toolchain`, `.python-version`, `.ruby-version` or a `ruby` directive in a `Gemfile`, `.java-version`, a `toolchain` line in `go.mod`, `gradlew`, `mvnw`, `deno.json` or `deno.jsonc` |
| 3. Lockfile | A lockfile or wrapper that only one tool produces | `pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, `turbo.json` |

The rung does not decide which candidate drives the workspace. The candidates'
[roles](#roles-composition-and-conflict) do. Rung rank settles two narrower
questions: which evidence gets recorded for a tool that several rungs name, and,
when two candidates hold the same role, whether a rung-1 declaration breaks the
tie.

A bare ecosystem marker is not evidence of a driver. These files identify a
language and no tool: `package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`,
`requirements.txt`, `setup.py`, `Gemfile`, `pom.xml`, `build.gradle`,
`build.gradle.kts`, `composer.json`, `mix.exs`, `pubspec.yaml`, `Package.swift`,
`stack.yaml`, `cabal.project`, and any `.sln`, `.csproj`, `.fsproj`, or
`.vbproj` file. None of them is a driver fingerprint. They feed only the
candidate list an ambiguity error prints, so the halt message can name the tools
that could plausibly have been meant.

Rung 3 covers the fingerprints in the driver registry. A sample:

| Tool | Fingerprint | Invoke form |
| --- | --- | --- |
| `pnpm` | `pnpm-lock.yaml` | `pnpm run {task}` |
| `cargo` | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo {task}` |
| `uv` | `uv.lock` | `uv run {task}` |
| `just` | `justfile`, `.justfile` | `just {task}` |
| `turbo` | `turbo.json` | `turbo run {task}` |

All 34 built-in drivers, with every fingerprint and invoke form, are in the
driver table on [Toolchains](/lattice/docs/toolchains). Two of them, `pip` and
`kotlin`, have no fingerprint, because no file on disk belongs to them alone.
They are reachable from rung 1 or rung 2 only.

## Roles: composition and conflict

Every driver declares one or more **roles**. Roles are what let several tools
coexist in one workspace, and they decide which one drives:

| Role | Rank | Examples |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift`, `stack`, `cabal` |
| Package manager | 2 | `pnpm`, `npm`, `yarn`, `bun`, `uv`, `poetry`, `pdm`, `pipenv`, `pip`, `bundler`, `nuget`, `pod`, `composer`, `dart` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake`, `mix` |

The candidate holding the highest-ranked role drives the workspace. A tool with
several roles competes with its highest one and no other. `deno` is a runtime, a
package manager, and a task runner, so it competes as a task runner. `bun` is a
runtime and a package manager, so it competes as a package manager. `mix` is
Elixir's package manager and task runner, so it competes as a task runner.

**Different roles compose.** A `.nvmrc` (node, a runtime) beside a
`pnpm-lock.yaml` (pnpm, a package manager) is not a conflict. pnpm drives, and
node stays in the resolved engine map so it can still be version-checked or
provisioned. A `turbo.json` over a `pnpm-lock.yaml` resolves to turbo the same
way, because a task runner outranks a package manager.

**The same role conflicts.** Two package managers in one workspace, say
`pnpm-lock.yaml` and `bun.lockb`, leave Lattice nothing to prefer, so it halts:

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Candidate drivers: bun, pnpm
Declare the driver in lattice.json, under this workspace:
  "engines": { "bun": ">=0.0.0" }
```

Two build tools, say `stack.yaml.lock` beside `cabal.project.freeze`, behave the
same. A declaration naming exactly one of the tied candidates resolves it, even
with the other's lockfile still on disk. A declaration for a *lower*-ranked role
does not: declaring `node` in a workspace that has a `pnpm-lock.yaml` still
resolves to pnpm, because pnpm's role outranks node's.

**A runtime cannot drive a named task alone.** There is no universal
`node build` or `python test`, because a runtime runs a file rather than a task
by name. When every candidate in a workspace is a runtime, Lattice halts as if
it had found nothing.

## When Lattice halts

An `auto` workspace with no unambiguous driver raises an ambiguity error and
stops the run before any task starts. Here it is for a workspace containing only
a `package.json`:

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

With no ecosystem marker either, as in a workspace holding only a `.nvmrc`, the
candidate list is empty and the message says so:

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Lattice detected no driver. The directory holds no lockfile, no wrapper, and no native declaration.
Declare the driver in lattice.json, under this workspace:
  "auto": false, "scripts": { "build": "<command>" }
```

The suggested line differs between the two because a runtime cannot drive tasks.
Where a candidate tool exists, Lattice names one, and declaring it resolves the
halt. Where none does, no `engines` entry would help, so the message suggests
declaring the commands instead.

Either error clears in one of three ways: declare the tool that should run the
tasks in that workspace's `engines`, declare it in the root `engines` map, or
set `auto: false` and write the commands yourself. The `>=0.0.0` in the first
suggestion is a placeholder that matches any version. See [Engines and
provisioning](/lattice/docs/engines) for a real constraint.

## `auto: false`

The ladder is how an `auto: true` workspace, the default, resolves a driver
without you declaring commands. `auto: false` turns off its consequences.
Lattice may still detect a driver, but it infers no command from one, and an
ambiguous or undetectable result never halts the run. Only `scripts` supplies
commands.

```json
{
  "workspaces": [
    { "name": "app", "path": "apps/app", "auto": false }
  ],
  "tasks": {
    "build": {}
  }
}
```

Such a workspace needs a `scripts` entry for every task it participates in.
Asking for one it does not list fails rather than being skipped:

```text
Error: workspace 'app' has "auto": false and declares no command for task 'build'. Add the command under this workspace's "scripts" map in lattice.json
```

The skip applies only to an `auto: true` workspace whose driver has no command
for a given task. See [Workspaces](/lattice/docs/workspaces) for the full
`scripts` and `auto` reference.

## What a driver can run

Resolving a driver is not the same as resolving a command. A driver falls into
one of two groups, and the group decides where a task's command comes from.

One group takes the task name on its command line. `cargo`, `go`, `make`,
`just`, `gradle`, and most of the table work this way, and the invoke form is the
command. `lattice run test` in a `cargo` workspace runs `cargo test` whether or
not such a target exists, and `cargo` reports the missing target rather than
Lattice.

Within that group, a task runner is treated differently for one thing:
`persistent: true`. `just`, `task`, `turbo`, `nx`, `rake`, and `mix` run the
tasks the repo declared to them, and `dev` is normally one of those, so a
persistent task infers a command there — `dev` in a `turbo.json` workspace
resolves to `turbo run dev`. The rest of the group cannot, because there is no
`cargo dev`, so a persistent task in a `cargo`, `go`, or `gradle` workspace
needs a `scripts` entry naming the command.

The other group reads its tasks out of a manifest. `npm`, `pnpm`, `yarn`, and
`bun` read `scripts` in `package.json`, and `deno` reads `tasks` in `deno.json`
or `deno.jsonc`. Such a driver can run only a script that manifest declares. A
requested task the manifest leaves out does not run in that workspace at all. The
task drops out of the graph, and the run carries on with the workspaces that do
declare it.

That skip is deliberate. A types-only package with no `build` script has nothing
to build, and an invented `npm run build` for it fails. Lattice used to invent
one.

A missing script and a mistyped one look the same from outside, so Lattice warns
when a manifest declares a script map without the task you asked for:

```text
warn web declares scripts but no "build", so the task was skipped. Did you mean "biuld"?
```

Lattice stays quiet when the manifest declares no scripts at all, and when the
workspace has `auto: false`. See [Errors](/lattice/docs/errors#a-task-was-skipped-because-the-manifest-declares-no-such-script)
for every shape that warning takes, and for what Lattice says about a manifest
it cannot parse.

A `scripts` entry in `lattice.json` overrides both groups. It supplies the
command directly, so it is how you run a task in a workspace whose manifest does
not declare it.
