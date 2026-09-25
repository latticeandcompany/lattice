---
title: Adopting Lattice
description: Add Lattice to a repo that already builds, and shape what it runs and caches.
group: Guides
order: 1
---

# Adopting Lattice

Do this in three passes. Get `lattice run` calling the commands you already run.
Then narrow what each task caches. Pin tool versions last, and only where you
need the guarantee.

Nothing here requires you to stop using what the repo has now. Every
`package.json` script, `Makefile` and CI step keeps working until you point CI
at `lattice run` too.

## Start with `lattice init`

```sh
lattice init
```

`init` writes a `lattice.json` holding only what discovery would not find on its
own, then lists every workspace it now sees, with the tool that runs it and the
tasks it can run:

```text
4 workspaces:
  @acme/ui   pnpm     build lint test
  web        pnpm     build dev lint test
  api        go       build test check lint format clean
  jvm        gradle   build assemble check test clean run
```

Read that table against the repo. Most of it comes from files the repo already
has: the members of `pnpm-workspace.yaml`, the lockfile at the root, each
package's scripts, the Go module and the Gradle build. What `init` proposes to
write is only the rest:

- **Directories no workspace file names**, such as `services/api` and `jvm`
  here, as entries in `workspaces`. Two or more siblings that are every project
  under their parent become one glob (`services/*`), so the next service there
  is picked up without an edit.
- **Tool versions the root already pins**, in `.nvmrc`, `.tool-versions`,
  `rust-toolchain.toml` and the rest, as `engines` entries.
- **Task settings a `turbo.json` spells out**, when its directory is one of the
  proposed workspaces.

On a terminal you confirm each list. Besides `$schema` and `latticeVersion`, the
result is often a handful of lines:

```json
{
  "workspaces": ["services/api", "jvm"],
  "engines": { "node": "22.11.0" }
}
```

A Gradle build is one workspace however many subprojects it has: its tool orders
them, and two Gradle processes on one build contend for the same locks. List the
build's root. See [Workspaces](/lattice/docs/workspaces#one-build-is-one-workspace).

## Check the resolved commands

```sh
lattice run build --dry-run
```

```text
❖ lattice  dry run · build
  → @acme/ui:build  pnpm run build
  → api:build  go build ./...
  → jvm:build  ./gradlew build
  → web:build  pnpm run build
```

Each line is `workspace:task` and the command that workspace will run. Read it
against what you run in that directory today. Try the other verbs you use:
`lattice run lint --dry-run` shows `cargo clippy`, `go vet ./...`, or a
package's own `lint` script, and leaves out workspaces with nothing to lint.

## Fix a workspace Lattice cannot place

A package with a `package.json` and no lockfile anywhere, or a repo part-way
between two package managers, has no single answer:

```text
error: Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

Name the tool the package actually uses, or commit its lockfile. The suggestion
in the message is only the first candidate. See [Driver
detection](/lattice/docs/drivers).

## Write out what Lattice cannot know

Put per-workspace settings under `overrides`, keyed by workspace name or by a
glob over paths:

```json
{
  "overrides": {
    "web": { "dependsOn": ["api"] },
    "api": { "tasks": { "deploy": "./deploy.sh" } },
    "tools/codegen": { "name": "codegen", "driver": "none", "tasks": { "build": "make proto" } }
  }
}
```

- `dependsOn` adds an edge no manifest can state, here a web app that needs the
  Go service built first.
- A task written as a string adds or replaces a command in that workspace. The
  other tasks keep coming from the manifest.
- `"driver": "none"` is for a directory whose every command you write yourself,
  such as a folder of shell scripts.

## Narrow what each task hashes

A task with no `inputs` hashes every file in its workspace that git does not
ignore. That is correct but coarse: a README edit invalidates a build. Declaring
`inputs` narrows it:

```json
{
  "tasks": {
    "build": { "inputs": ["src/**", "public/**", "package.json", "vite.config.ts"] }
  }
}
```

The root `tasks` map applies to every workspace that has the task. Where one
workspace differs, set it in that workspace's override; fields layer, so an
override that names `outputs` changes only `outputs`.

`build` already caches the tool's build directory (`dist/**` for JavaScript and
Python packages, `**/build/**` for Gradle, `**/target/**` for Maven,
`**/bin/Debug/**` and `**/bin/Release/**` for .NET). A framework that writes elsewhere, such as `.next/`, needs its own
`outputs`.

A file above every workspace, such as a shared `tsconfig.base.json`, goes in
`globalDependencies`, and repo-wide variables in `globalEnv`. See
[Caching](/lattice/docs/caching).

To keep a task running every time while you work out what it reads and writes,
set `"cache": false` on it.

## Pin tool versions last

Nothing above pins a version, so each tool is whatever is on `PATH`. Add
`engines` when a version has to hold on every machine and CI runner, or when
Lattice should install a tool into `.lattice/toolchains` rather than the machine
having it globally. A root entry applies to every workspace, and an override
changes it for some. See [Pinning tool versions](/lattice/docs/pinning-tool-versions).

## Where to go next

- [Workspaces](/lattice/docs/workspaces) for discovery, names and dependencies.
- [Caching](/lattice/docs/caching) for what is hashed and what counts as a hit.
- [Nested repos](/lattice/docs/nested-repos) for a workspace with its own task
  runner.
- [Continuous integration](/lattice/docs/continuous-integration) for the CI
  step.
- [Troubleshooting](/lattice/docs/troubleshooting) for symptom-to-fix.
