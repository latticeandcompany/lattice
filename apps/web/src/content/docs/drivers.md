---
title: Driver detection
description: How Lattice picks the tool that runs a workspace's tasks, and when it stops to ask instead.
group: Concepts
order: 4
---

# Driver detection

A task named `build` has to become a real shell command. The tool that supplies
that command is the workspace's **driver**.

The evidence that identifies a language rarely identifies a driver. A lone
`package.json` says JavaScript. It does not say whether `build` means
`pnpm run build`, `yarn build`, `npm run build`, or `bun run build`, and those
commands can behave differently. Lattice looks for evidence that names one tool,
and halts when it finds none.

## The evidence ladder

For each workspace, Lattice gathers candidate tools from three kinds of
evidence. A tool named by more than one kind keeps the highest-ranked:

| Rung | Evidence | Examples |
| --- | --- | --- |
| 1. Declaration | A driver named in the workspace's resolved `engines` map — its own entries merged over the root's | `"engines": { "pnpm": ">=8" }` |
| 2. Native file | A file the developer wrote to pin a tool | `packageManager` in `package.json`, `.tool-versions`, a `[tools]` table in `mise.toml`, `.nvmrc`, `rust-toolchain.toml`, `.python-version`, `.ruby-version` or a `ruby` directive in a `Gemfile`, `.java-version`, a `toolchain` line in `go.mod`, `gradlew`, `mvnw`, `deno.json` |
| 3. Lockfile | A lockfile or wrapper only one tool produces | `pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, `turbo.json` |

The rung does not decide which candidate drives the workspace — the candidates'
[roles](#roles-composition-and-conflict) do. Rung rank settles two narrower
questions: which evidence gets recorded for a tool that several rungs name, and,
when two candidates hold the same role, whether a rung-1 declaration breaks the
tie.

A bare ecosystem marker is not evidence of a driver. A `package.json` with no
lockfile and no `packageManager` field identifies the JavaScript ecosystem, and
so do `Cargo.toml`, `go.mod`, `pyproject.toml`, `requirements.txt`, `setup.py`,
`Gemfile`, `pom.xml`, `build.gradle`, `build.gradle.kts`, `composer.json`,
`mix.exs`, `pubspec.yaml`, `Package.swift`, `stack.yaml`, `cabal.project`, and a
.NET project or solution file (`.sln`, `.csproj`, `.fsproj`, `.vbproj`) for
theirs. None of them is a driver fingerprint. They feed only the candidate list
an ambiguity error prints, so the halt message can name the tools that could
plausibly have been meant.

Rung 3 covers the fingerprints in the driver registry. A sample:

| Tool | Fingerprint | Invoke form |
| --- | --- | --- |
| `pnpm` | `pnpm-lock.yaml` | `pnpm run {task}` |
| `cargo` | `Cargo.lock`, `rust-toolchain[.toml]` | `cargo {task}` |
| `uv` | `uv.lock` | `uv run {task}` |
| `just` | `justfile`, `.justfile` | `just {task}` |
| `turbo` | `turbo.json` | `turbo run {task}` |

All 34 built-in drivers, with every fingerprint and invoke form, are in the
driver table on [Toolchains](/lattice/docs/toolchains). Two of them — `pip` and
`kotlin` — have no fingerprint, because no file on disk belongs to them alone;
they are reachable from rung 1 or rung 2 only.

## Roles: composition and conflict

Every driver declares one or more **roles**. Roles are what let several tools
coexist in one workspace, and they decide which one drives:

| Role | Rank | Examples |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift` |
| Package manager | 2 | `pnpm`, `npm`, `yarn`, `bun`, `uv`, `poetry`, `nuget` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake` |

The candidate holding the highest-ranked role drives the workspace. A tool with
several roles competes with its highest one and no other: `deno` is a runtime, a
package manager, and a task runner, so it competes as a task runner; `bun`
competes as a package manager; `mix` is Elixir's package manager and task runner,
so it competes as a task runner.

**Different roles compose.** A `.nvmrc` (node, a runtime) beside a
`pnpm-lock.yaml` (pnpm, a package manager) is not a conflict. pnpm drives, and
node stays in the resolved engine map so it can still be version-checked or
provisioned. A `turbo.json` over a `pnpm-lock.yaml` resolves to turbo the same
way: a task runner outranks a package manager.

**The same role conflicts.** Two package managers in one workspace —
`pnpm-lock.yaml` and `bun.lockb` — leave Lattice nothing to prefer, so it halts:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: bun, pnpm
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "bun": ">=0.0.0" }
```

Two build tools — `stack.yaml.lock` beside `cabal.project.freeze` — behave the
same. A declaration naming exactly one of the tied candidates resolves it, even
with the other's lockfile still on disk. A declaration for a *lower*-ranked role
does not: declaring `node` in a workspace that has a `pnpm-lock.yaml` still
resolves to pnpm, because pnpm's role outranks node's.

**A runtime cannot drive a named task alone.** There is no universal `node build`
or `python test` — a runtime runs a file, not a task by name. When every
candidate in a workspace is a runtime, Lattice halts as if it had found nothing.

## When Lattice halts

An `auto` workspace with no unambiguous driver raises an ambiguity error and
stops the run. Here it is for a workspace containing only a `package.json`:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: pnpm, npm, yarn, bun
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "pnpm": ">=0.0.0" }
```

With no ecosystem marker either — a workspace holding only a `.nvmrc`, say — the
candidate list is empty and the message says so:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "node": ">=0.0.0" }
```

Fix either by declaring the tool that should run the tasks — a package manager,
build tool, or task runner — in that workspace's `engines`, or in the root
`engines` map, or by setting `auto: false` and writing the commands yourself.
Naming a runtime does not resolve it; the suggested line falls back to `node`
whenever the candidate list is empty, so replace it with the tool you actually
run. The `>=0.0.0` in the suggestion is a placeholder too — see [Engines and
provisioning](/lattice/docs/engines) for a real constraint.

## `auto: false`

The ladder is how an `auto: true` workspace (the default) resolves a driver
without you declaring commands. `auto: false` turns off its consequences:
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
Asking for one it doesn't list fails rather than being skipped — the silent skip
applies only to `auto: true` workspaces whose detected driver has no command for
a given task:

```text
Error: workspace 'app' is "auto": false but declares no command for task 'build'; add it under this workspace's "scripts" map in lattice.json
```

See [Workspaces](/lattice/docs/workspaces) for the full `scripts` and `auto`
reference.
