---
title: Driver detection
description: How Lattice picks the tool that runs a workspace's tasks, and when it stops to ask instead.
group: Concepts
order: 4
---

# Driver detection

A workspace's task `build` has to become a real shell command. Lattice calls
the tool that supplies that command the workspace's **driver**. Finding it is
harder than it looks, because the evidence that tells you the *language* is
rarely the evidence that tells you the *driver*.

A lone `package.json` says "this is JavaScript." It does not say whether
`build` means `pnpm run build`, `yarn build`, `npm run build`, or
`bun run build`. Those commands can behave differently, and picking one for
the developer would hand them a tool they never chose. So Lattice never
guesses from a language marker alone — it looks for evidence that names one
tool specifically, and if it can't find any, it stops and asks.

## The evidence ladder

For each workspace, Lattice climbs a fixed ladder and stops at the first rung
that produces a single, unambiguous tool:

| Rung | Evidence | Example |
| --- | --- | --- |
| 1. Declaration | A tool named in this workspace's (or the root's) `engines` in `lattice.json` | `"engines": { "pnpm": ">=8" }` |
| 2. Native file | A file the developer authored to pin a tool, not Lattice | `packageManager` in `package.json`, `.tool-versions`, `.nvmrc`, `rust-toolchain.toml`, `./gradlew` |
| 3. Lockfile | A lockfile or wrapper only one tool produces | `pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, `turbo.json` |
| 4. Nothing unambiguous | — | halt and ask |

Rung 1 always wins outright, because it's what you told Lattice yourself.
Below that, native files and lockfiles are only ever used to disambiguate
when two tools would otherwise tie — see
[roles](#roles-composition-and-conflict) below.

A **bare generic marker is never enough on its own.** A lone `package.json`
with no lockfile and no `packageManager` field identifies the JavaScript
ecosystem, not a driver; the same goes for a lone `Cargo.toml`, `go.mod`,
`pom.xml`, `pyproject.toml`, `requirements.txt`, `Gemfile`, `composer.json`,
`mix.exs`, `pubspec.yaml`, `Package.swift`, `stack.yaml`, `cabal.project`, or
`.csproj`. None of these appear as fingerprints in the driver registry — they
only feed the *list of candidates* an ambiguity error suggests, so the halt
message names the tools that could plausibly have been meant.

Rung 2 covers files like `.tool-versions` (asdf/mise), `mise.toml`'s
`[tools]` table, a `go.mod` `toolchain` line, a `ruby "3.2.0"` directive in a
`Gemfile`, and version-pin files (`.python-version`, `.java-version`,
`.ruby-version`). Rung 3 covers the rest of the fingerprints in the driver
registry — a handful, to be concrete:

| Tool | Fingerprint | Invoke form |
| --- | --- | --- |
| `pnpm` | `pnpm-lock.yaml` | `pnpm run {task}` |
| `cargo` | `Cargo.lock`, `rust-toolchain[.toml]` | `cargo {task}` |
| `uv` | `uv.lock` | `uv run {task}` |
| `just` | `justfile`, `.justfile` | `just {task}` |
| `turbo` | `turbo.json` | `turbo run {task}` |

The full set of 34 built-in drivers, with every fingerprint and invoke form, is
the built-in driver table on [Toolchains](/lattice/docs/toolchains). Two of them
(`pip` and `kotlin`) have no fingerprint at all — no file on disk belongs to
them alone — so they are reachable only from rung 1 or rung 2.

## Halting instead of guessing

When no rung produces a single tool, Lattice raises an ambiguity error rather
than picking one. For an `auto` workspace (the default — see
[Workspaces](/lattice/docs/workspaces)) this halts the run. Here is a real
one, from a workspace containing only a `package.json`:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: pnpm, npm, yarn, bun
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "pnpm": ">=0.0.0" }
```

If not even a generic ecosystem marker is present — say, the workspace has
only a `.nvmrc` and nothing else — the candidate list is empty and the
message says so plainly instead of listing tools that aren't actually there:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "node": ">=0.0.0" }
```

That second case is not a bug in the ladder — it's rung 4 doing its job. A
`.nvmrc` names a JavaScript *runtime*, and a runtime alone can't drive named
tasks (more on this below), so Lattice halts exactly as it would with no
evidence at all.

Either error is fixed the same way: add the suggested `engines` entry (or the
tool you actually want) to that workspace, or to the root `engines` map, in
`lattice.json`. That declaration is rung 1, so it settles the question on the
next run. The version constraint in the suggestion (`>=0.0.0`) is a
placeholder — replace it with a real one once you're declaring the engine on
purpose; see [Engines and provisioning](/lattice/docs/engines).

## Roles: composition and conflict

Every driver carries one or more **roles** — what kinds of job it does — and
roles are what let more than one tool coexist in a workspace without triggering
an ambiguity error. From lowest to highest driving rank:

| Role | Rank | Examples |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift` |
| Package manager | 2 | `pnpm`, `npm`, `yarn`, `bun`, `uv`, `poetry`, `nuget` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake` |

Some tools do several of these jobs. `deno` is a runtime, a package manager,
and a task runner; `bun` is a runtime and a package manager; `mix` is Elixir's
package manager and its task runner. A tool like that competes with its
**highest-ranked** role and no other — `deno` as a task runner, `bun` as a
package manager. That is why a `.nvmrc` next to a `bun.lockb` resolves to bun
instead of deadlocking two runtimes: bun's package-manager role outranks node's
runtime role, so the two compose rather than conflict.

**Different roles compose into a stack.** A `.nvmrc` (node, a runtime) next to
a `pnpm-lock.yaml` (pnpm, a package manager) is not a conflict: pnpm drives,
because a package manager outranks a bare runtime, and node stays on record in
the resolved engine map so it can still be provisioned. The same logic is why
a `turbo.json` sitting over a `pnpm-lock.yaml` resolves to turbo — a task
runner outranks a package manager, so turbo drives and schedules pnpm
underneath it.

**The same role conflicts.** Two package managers in one workspace —
`pnpm-lock.yaml` and `bun.lockb`, say — give Lattice no way to prefer one over
the other, so it halts:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: bun, pnpm
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "bun": ">=0.0.0" }
```

The same happens with two build tools, e.g. `stack.yaml.lock` next to
`cabal.project.freeze`. A declaration breaks the tie the same way it settles a
rung-4 halt: if `engines` names exactly one of the tied candidates, that
becomes the driver even though a lockfile for the other is still on disk.

**A pure runtime can never drive a named task by itself.** There is no
universal `node build` or `python test` — a runtime only knows how to run a
file, not a task by name. So if the only evidence found in a workspace is
runtime-level (only a `.nvmrc`, only a `.python-version`, and nothing above
it in the rank table), Lattice treats that exactly like no evidence at all
and raises the same "no task driver could be detected" halt shown above.

## The manual override

Rungs 1–3 are how `auto: true` workspaces (the default) resolve a driver
without you declaring commands yourself. Setting `auto: false` on a workspace
turns off the *consequences* of that ladder: Lattice may still notice a
driver in passing, but it never infers a command from it and an ambiguous or
undetectable result never halts the run — only `scripts` supplies commands
for a manual workspace.

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

A workspace declared this way needs its own `scripts` map naming the exact
command for every task it participates in. Ask for a task this workspace
doesn't list in `scripts`, and it fails outright rather than being silently
skipped — that silent skip is only for `auto: true` workspaces whose detected
driver doesn't apply to a given task:

```text
Error: workspace 'app' is "auto": false but declares no command for task 'build'; add it under this workspace's "scripts" map in lattice.json
```

See [Workspaces](/lattice/docs/workspaces) for the full `scripts` and `auto`
reference.
