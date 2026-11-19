---
title: Workspaces
description: How Lattice finds the directories it runs tasks in, what it names them, and how it works out which depends on which.
group: Concepts
order: 1
---

# Workspaces

A workspace is a directory Lattice runs tasks in: a JavaScript package, a Rust
crate, a Go module, a Gradle build. Tasks run per workspace and results are
cached per workspace, so the workspace is the unit everything else in Lattice
is counted in.

You rarely list them. Most monorepos already say what their packages are, in a
file their package manager or build tool reads, and Lattice reads the same
file.

## Where the list comes from

Lattice builds the list in four steps.

**The repo root's workspace files.** Each one names members the way its tool
does:

| File | Members |
| --- | --- |
| `package.json` | `workspaces`, an array or `{ "packages": [...] }` |
| `pnpm-workspace.yaml` | `packages` |
| `Cargo.toml` | `[workspace] members`, minus `exclude` |
| `go.work` | every `use` directory |
| `pyproject.toml` | `[tool.uv.workspace] members`, minus `exclude` |
| `deno.json` | `workspace` |

A `!` entry removes what the other entries matched, and a member has to hold
its ecosystem's manifest to count, so `packages/*` does not sweep in a
`packages/notes` directory. A pattern does not walk through `node_modules`,
`dist`, `build`, `out`, `target`, `vendor`, `venv` or `__pycache__`, but a
directory it names directly is a member whatever it is called: `packages/*`
includes `packages/build`.

**`workspaces` in `lattice.json`.** Directories or globs added to the list:
the Go service beside a JavaScript monorepo, a Python tool no workspace file
knows about.

```json
{ "workspaces": ["services/*", "tools/codegen"] }
```

**`exclude` in `lattice.json`.** Globs removed from both, for the fixture package
or the retired app the workspace file still names.

**Otherwise, the root.** A repo that names no workspace anywhere is a single
project, and the root is its one workspace. A plain Gradle build or a lone Cargo
crate needs no `lattice.json` entries at all.

## Why the tool's own list

A crawl for manifests has to decide what counts as a package, and every repo has
directories that look like one and are not: a fixtures tree with its own
`package.json`, a vendored dependency, an example nobody builds. The workspace
file is where the repo already decided. Reading it means a package added the
usual way is picked up with no edit to `lattice.json`, and a directory the
package manager ignores is ignored here too.

A `lattice.json` glob is the escape hatch for what no workspace file covers, and
it is bounded the same way: a directory it matches that holds nothing Lattice
recognizes is skipped, not turned into a workspace that fails.

## One build is one workspace

Only the root's workspace files are expanded. A directory Lattice treats as a
workspace stays one workspace however it is organised inside.

That matters most for a Gradle, Maven or .NET build. Its `settings.gradle`,
`<modules>` or `.sln` lists subprojects, and the build tool orders them,
shares a daemon across them, and holds locks on their output directories. Two
`./gradlew` processes on one build at the same time wait on each other's locks
or step on each other's outputs. So Lattice runs a Gradle build as one workspace
at its root, and `lattice run test` becomes one `./gradlew test` that Gradle
parallelizes itself. List the build's root, not its subprojects.

The same holds for a nested monorepo with its own task runner, or a Cargo
workspace in a subdirectory: list the directory, and its own tool builds it. See
[Wrap a repo that has its own task runner](/lattice/docs/nested-repos).

## The root is not a workspace when it names members

A root `package.json` with `workspaces`, or a `Cargo.toml` with only
`[workspace]`, organises the repo rather than being a package in it, so it is
not a workspace. Two roots are both: a `Cargo.toml` with its own `[package]`
beside `[workspace]`, and a `pyproject.toml` with its own `[project]` beside
`[tool.uv.workspace]`. To run the root's own scripts, add `"."` to
`workspaces`.

## Names come from the manifest

A workspace is named by its manifest: the `name` in `package.json` (scope and
all, so `@acme/ui`), the crate name in `Cargo.toml`, the project name in
`pyproject.toml`, the `name` in `composer.json`. A workspace with none of those
(a Go module, a Gradle build) takes its directory's name.

Two workspaces that come out with the same name, `apps/web` and `sites/web`, are
both named by their paths instead. To choose a name yourself, set it in
`overrides`:

```json
{ "overrides": { "packages/ui": { "name": "ui" } } }
```

Names are what `--filter` matches, what `overrides` keys and `dependsOn` refer
to, and what the run output labels lines with. A scoped name such as `@acme/ui`
is an `overrides` key as written, slash and all.

## Dependencies come from the manifest too

Which workspace builds before which is read from the manifests:

| Ecosystem | An edge when a workspace names another in |
| --- | --- |
| npm, pnpm, yarn, bun | `dependencies`, `devDependencies`, `peerDependencies`, `optionalDependencies`, whatever the version spec |
| Cargo | a `path` dependency, directly or through `[workspace.dependencies]` |
| Go | a `require` of its module path, or a `replace` to its directory |
| Python | `[project] dependencies`, optional dependencies, dependency groups, or a uv `path` source |
| Composer | `require` or `require-dev` |

The one edge no manifest can state is between languages: the web app that serves
a bundle the Rust crate compiles to WebAssembly. Write that one yourself:

```json
{ "overrides": { "web": { "dependsOn": ["wasm-core"] } } }
```

An edge is a fact about the repo, not a schedule. It orders tasks only where a
task asks for it with `^`. `build` does by default (`dependsOn: ["^build"]`),
so builds run in dependency order; `lint` does not, so linting stays parallel.
[Task graph](/lattice/docs/task-graph) covers the two tokens.

## The workspace directory is a boundary

Almost everything Lattice does to a task is scoped to its workspace directory.
The input walk stops there. `outputs` globs are resolved from there. A cache hit
clears what those globs match inside it before unpacking the stored artifact.

So a workspace has to be inside the repo. A `workspaces` entry that is absolute
or climbs out with `..` fails validation, and a directory that resolves outside
the repo through a symlink is refused at discovery:

```text
error: workspace 'app' resolves to /elsewhere/app, which is outside the repo root. A workspace directory has to be inside the repo
```

A file above every workspace, such as a base `tsconfig.json`, is read by tasks
in several workspaces and is named by no workspace-relative pattern.
`globalDependencies` covers it. See [Caching](/lattice/docs/caching).

## Settings for one workspace

`overrides` holds everything about particular workspaces, keyed by name or by a
glob over paths:

```json
{
  "overrides": {
    "web": {
      "engines": { "node": ">=24" },
      "tasks": { "e2e": { "run": "playwright test", "cache": false } }
    },
    "services/*": { "engines": { "go": ">=1.23" } }
  }
}
```

A different tool version for one workspace, a task its manifest does not
declare, a command written out by hand, a driver Lattice could not work out.
Every field is in [Configuration](/lattice/docs/configuration#overrides).

## Where to look next

Which tool runs each workspace's tasks is [Driver
detection](/lattice/docs/drivers). How a task name becomes one node per
workspace is [Task graph](/lattice/docs/task-graph). Every error this page shows
is in [Errors](/lattice/docs/errors).
