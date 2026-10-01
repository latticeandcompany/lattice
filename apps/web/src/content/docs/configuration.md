---
title: Configuration
description: Field reference for lattice.json, with types, defaults, and what an invalid value does.
group: Reference
order: 1
---

# Configuration

`lattice.json` at the repo root holds what Lattice cannot read from the repo's
own manifests. Workspaces, the dependencies between them and their tasks come
from `package.json`, `Cargo.toml`, `go.mod` and the rest; this file adds
directories those do not name, settings for tasks, per-workspace overrides,
tool versions, and cache settings. For what the fields mean in practice, see
[Workspaces](/lattice/docs/workspaces), [Task graph](/lattice/docs/task-graph),
[Caching](/lattice/docs/caching), and [Engines and
provisioning](/lattice/docs/engines).

## Top-level fields

Every key is optional, so `{}` is a complete `lattice.json` for a repo whose
workspace files already say everything.

| Field | Type | Default |
| --- | --- | --- |
| [`$schema`](#schema) | `string` | none |
| [`latticeVersion`](#latticeversion) | `string` | none |
| [`workspaces`](#workspaces) | array of `string` | `[]` |
| [`exclude`](#exclude) | array of `string` | `[]` |
| [`engines`](#engines) | engine map | `{}` |
| [`globalDependencies`](#globaldependencies) | array of `string` | `[]` |
| [`globalEnv`](#globalenv) | array of `string` | `[]` |
| [`tasks`](#tasks) | object of task name to task object | `{}` |
| [`overrides`](#overrides) | object of key to override object | `{}` |
| [`settings`](#settings) | settings object | all defaults |

## Where the file lives

Every command that reads config walks up from the current directory to the
nearest `lattice.json` and treats that directory as the repo root. With none in
the current directory or any parent, the command fails before reading anything
else:

```text
error: no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

## A 1.x config

A config whose `workspaces` holds objects was written for Lattice 1.x. It is
refused before anything else is checked:

```text
error: lattice.json is written for Lattice 1.x, which listed every workspace as an object with a name and a path. Run `lattice migrate` to rewrite it for this version
```

See [Upgrading to 2.0](/lattice/docs/upgrading#upgrading-to-20).

## `$schema`

```json
{ "$schema": ".lattice/schema.json" }
```

A reference to a copy of Lattice's JSON Schema written next to your config, so
editors validate and autocomplete `lattice.json` as you type. Lattice never
reads the value. Every command that loads the config rewrites
`.lattice/schema.json` when it is missing or differs from the binary's own copy
(line endings aside), so an upgrade brings the new schema with it. Commit the
file. The rest of `.lattice/` is per-machine and ignored.

## Unknown and repeated keys

A key Lattice does not recognize fails the load before any task is scheduled:

```text
error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: run, dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

The message names the key, the object it sits in (`tasks.build`,
`overrides.web`, `engines.node`, or the top level), its position, the closest
accepted field, and every field accepted there.

A key written twice in `tasks`, `engines`, `overrides`, an override's `engines`
or an override's `tasks` fails too, because the second would silently replace
the first:

```text
error: duplicate key `build` in tasks (lattice.json line 8, column 3)
Keep one of them: the second replaces the first, so only the last would take effect
```

## `latticeVersion`

```json
{ "latticeVersion": "2.0.0" }
```

The version of Lattice this repo runs. `lattice upgrade` writes it. See
[Upgrading](/lattice/docs/upgrading) for how a binary Lattice installed hands
over to the pinned version.

## `workspaces`

```json
{ "workspaces": ["services/*", "tools/codegen"] }
```

Directories, or globs of them, relative to the repo root, that are workspaces
on top of the ones the root's own workspace files name: `package.json`
`workspaces`, `pnpm-workspace.yaml`, Cargo's `[workspace]`, `go.work`, uv's
`[tool.uv.workspace]`, and deno's `workspace`.

- `*` matches within one directory level and `**` crosses levels.
  `"apps/*"` does not reach `apps/web/admin`.
- `"."` makes the repo root a workspace.
- A glob skips hidden and gitignored directories and `node_modules`, `target`,
  `dist`, `build`, `out`, `vendor`, `venv` and `__pycache__`. A literal path is
  looked up as written.
- A directory a glob matches that holds nothing Lattice recognizes is skipped
  without comment.
- A directory named here is one workspace whatever is inside it: a nested
  monorepo, a Gradle build with subprojects, a Cargo workspace. Only the repo
  root's workspace files are expanded into members.

With no members from the root's workspace files and no `workspaces`, the root
alone is the workspace.

| Problem | Error |
| --- | --- |
| An entry that matches no directory | `` `workspaces` entry 'tolls/*' matches no directory. Check the spelling, or remove the entry `` |
| An absolute or drive-prefixed entry | `` `workspaces` entry '/srv/app' is not relative to the repo root. ... `` |
| An entry that climbs out with `..` | `` `workspaces` entry '../x' points outside the repo root. Every workspace has to be inside the repo `` |
| Whitespace around a directory name | `` `workspaces` entry 'a/ b' has leading or trailing whitespace around a directory name. ... `` |
| An invalid glob | `` `workspaces` entry 'a/[' is not a valid glob `` |
| A directory that resolves outside the repo through a symlink | `workspace '<path>' resolves to <target>, which is outside the repo root. ...` |

## `exclude`

```json
{ "exclude": ["packages/legacy", "examples/**"] }
```

Globs of workspace paths to leave out, whether a workspace file or
`workspaces` named them. The same path rules as `workspaces` apply.

## Workspace names

A workspace takes the name its manifest gives it: `package.json` `name`, scope
included (`@acme/ui`); then the crate name from `Cargo.toml`; then the project
name from `pyproject.toml`; then `composer.json` `name`; then the directory's
name. Two workspaces that derive the same name are both named by their relative
paths. `overrides.<key>.name` sets one explicitly, and two names set that way
may not collide:

```text
error: two workspaces are named 'web'. Give each a different `name` in `overrides`
```

## `engines`

```json
{
  "engines": {
    "node": ">=22",
    "protoc": {
      "version": ">=25.0.0",
      "versionCmd": "protoc --version",
      "installCmd": "curl -fsSL https://example.com/protoc.sh | sh -s -- \"$LATTICE_TOOLCHAIN_DIR\""
    }
  }
}
```

Tool versions every workspace runs with. An override's `engines` wins per name.
Each value is a constraint string or an object:

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `version` | `string` | none | A constraint: `">=13.0.0"`, `"^1.75"`, or a bare `"22.11.0"`, read as `^22.11.0` |
| `versionCmd` | `string` | the built-in rule | A command that prints the version |
| `installCmd` | `string` | none | A command that installs the tool into `$LATTICE_TOOLCHAIN_DIR`. Its presence selects provisioning |
| `bin` | `string` | `"bin"` | The directory inside the install to put on `PATH`. Relative, non-empty, inside the install |

The string form works only for the 40 engines in the [toolchain
reference](/lattice/docs/toolchains#well-known-engines); any other name needs the
object form with `versionCmd`:

```text
error: engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
```

An absolute or escaping `bin` fails the load, because it would put a directory
Lattice never installed at the front of every task's `PATH`:

```text
error: engine 'node' in overrides.web has a `bin` of '/usr/bin', which is not relative to the toolchain install. Write `bin` as a path inside the toolchain directory, like "bin"
```

The shape of the value decides whether Lattice checks the host tool, installs
its own, or does nothing. See [Engines and provisioning](/lattice/docs/engines).

## `globalDependencies`

```json
{ "globalDependencies": ["tsconfig.base.json", "proto/**"] }
```

Globs relative to the **repo root** whose contents are hashed into every task's
cache key. A task's `inputs` are relative to its own workspace, so this is the
only way to cover a file above one. Editing anything it matches makes every
task miss, so list only what really crosses workspaces.

## `globalEnv`

```json
{ "globalEnv": ["NODE_ENV"] }
```

Environment variable names whose values are hashed into every task's key.
Values come from the environment Lattice runs in; tasks inherit them anyway, so
they are not re-exported.

## `tasks`

```json
{
  "tasks": {
    "build": { "ignore": ["**/*.test.*"] },
    "test": { "dependsOn": ["build"], "timeout": "10m" }
  }
}
```

How a task behaves in every workspace that has it. A task does not have to be
listed here to run: any task a workspace's own files define can be run by name.
A task listed here that no workspace has creates nothing.

| Field | Type | Description |
| --- | --- | --- |
| `run` | `string` | The command. Only in an override; in this map it fails the load |
| `dependsOn` | array of `string` | `^build` is `build` in each workspace this one depends on; a bare `build` is `build` in the same workspace |
| `inputs` | array of `string` | Workspace-relative globs hashed into the cache key. Without it, every file in the workspace that git does not ignore |
| `outputs` | array of `string` | Workspace-relative globs stored on success and restored on a hit. Replaces the tool's default |
| `ignore` | array of `string` | Globs subtracted from what `inputs` matched |
| `env` | array of `string` | Variable names whose values are hashed. A set one is exported to the task |
| `persistent` | `boolean` | A long-running task such as a dev server. Never cached, and nothing may depend on it |
| `cache` | `boolean` | `false` always runs the task |
| `timeout` | `string` or integer | `"90s"`, `"10m"`, `"1h"`, or whole seconds, up to 365 days. On overrun the task's process group gets `SIGTERM`, then `SIGKILL` five seconds later, and the task fails with a `timed out after ... and was stopped` warning. Ignored on a persistent task |

### Defaults and layering

Each task's settings are stacked from four layers. A later layer replaces only
the fields it sets, and a list replaces rather than extends:

1. Lattice's defaults for the task's name:

   | Task | Default |
   | --- | --- |
   | `build` | `dependsOn: ["^build"]` |
   | `dev`, `start`, `serve`, `watch` | `persistent: true` |
   | `clean`, `deploy`, `publish`, `release` | `cache: false` |

2. For a `build` the workspace's own files define, the tool's build directory as
   `outputs`: `dist/**` for npm, pnpm, yarn, bun, deno, uv, poetry and pdm;
   `build/**` beside each Gradle build file; `target/**` beside each `pom.xml`; `**/bin/Debug/**` and
   `**/bin/Release/**` for .NET. Cargo, Go and uv workspace members get none.
3. This `tasks` map.
4. The workspace's overrides.

`"start": { "persistent": false }` turns a default off.

### Checks

```text
error: `tasks.build` has a `run`, but the root `tasks` map says how a task behaves in every workspace, not what it runs. Put the command under the workspace it belongs to: "overrides": { "<workspace>": { "tasks": { "build": "<command>" } } }
```

A task that lists itself in `dependsOn` fails the load. A `dependsOn` naming a
task no workspace has fails when the run is planned:

```text
error: task 'build' depends on 'codegne', but no workspace has a 'codegne' task
Did you mean `codegen`?
Tasks: build, codegen, lint, test
```

`timeout` takes `ms`, `s`, `m` and `h` (and their long spellings), or a whole
number of seconds. Under a second rounds up to one. More than 365 days is
rejected rather than clamped, and so is a fractional number like `1.5`; write
`"90s"`.

## `overrides`

```json
{
  "overrides": {
    "web": {
      "dependsOn": ["api"],
      "tasks": {
        "build": { "outputs": ["dist/**", ".astro/**"] },
        "e2e": { "run": "playwright test", "cache": false }
      }
    },
    "tools/codegen": { "name": "codegen", "driver": "none", "tasks": { "build": "make" } },
    "services/*": { "engines": { "go": ">=1.23" } }
  }
}
```

Settings for particular workspaces. A key is a **workspace name**, or a **glob
over workspace paths** when it is `"."` or contains a `/` or a glob character. A
top-level directory is matched by path as `"./tools"`.

| Field | Type | Description |
| --- | --- | --- |
| `name` | `string` | The name Lattice uses for the workspace, in place of its manifest's |
| `driver` | `string` | The tool that runs its tasks, when Lattice cannot tell or picks the wrong one. `"none"` for a workspace whose every task is written out under `tasks` |
| `engines` | engine map | Tool versions for these workspaces, over the root `engines` |
| `dependsOn` | array of `string` | Workspaces this one depends on beyond what its manifest says |
| `tasks` | object | A command string, or a task object whose `run` is the command and whose other fields layer over the root `tasks` map |

Several keys can match one workspace. They apply in the order written: `name`
and `driver` take the last value, `engines` merge per name, `dependsOn`
combine, and each task layers field by field. A task with a `run` replaces the
command Lattice found or adds a new task. A task object with no `run` changes
the settings of a task the workspace already has, and does nothing where it
has none.

| Problem | Error |
| --- | --- |
| A name key no workspace has | `` `overrides` key 'weeb' names no workspace ``, with the nearest name and the list |
| A path key that matches nothing | `` `overrides` key 'app/*' matches no workspace path `` |
| A driver Lattice does not know | `workspace 'gen' names driver 'pnmp', which Lattice does not know`, with the nearest driver and the list |
| A `dependsOn` naming no workspace | `workspace 'web' depends on 'apii', which is not a workspace`, with the nearest name |
| A workspace depending on itself | `` workspace 'web' lists itself in `dependsOn` `` |
| An empty `name` | `` `overrides.web.name` is empty `` |

## `settings`

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `maxCacheSize` | `string` | none | `"10GB"`, `"512MB"`, or bare bytes as a string (base 1024). Enforced after every run that writes, and used by `lattice prune` |
| `cacheDir` | `string` | `".lattice/cache"` | The cache directory, relative to the root. Inside the repo, and not the root itself |
| `loquacious` | `boolean` | `false` | Always verbose, as if `-v` were passed |
| `versionCheck` | `boolean` | `true` | `false` turns off the `latticeVersion` handover and nag |

`maxCacheSize` has to be a string. `cacheDir` is checked at load because
`lattice prune` deletes cache files in whatever directory it names:

```text
error: `settings.cacheDir` is '.', which is the repo root itself. Point it at a directory of its own, like ".lattice/cache" — `lattice prune` deletes cache archives and partial writes in whatever directory it names
```

## When each check runs

| Stage | What fails there |
| --- | --- |
| Finding the root | No `lattice.json` here or above |
| Parsing | A 1.x config, malformed JSON, unknown and repeated keys, wrong types, bad durations and sizes |
| Validation | Engine forms and `bin`, `cacheDir`, `workspaces` and `exclude` entries outside the repo, `run` in the root map, a root task depending on itself, an empty override `name` |
| Discovery | A `workspaces` entry matching nothing, an override key matching nothing, an unknown `driver`, a workspace Lattice cannot pick a tool for, a `dependsOn` naming no workspace, a name clash |
| Planning | A task no workspace has, a `dependsOn` naming no task, a dependency on a persistent task, a cycle |
| First run | Engine checks and installs |

Nothing runs when any of these fails.

## Complete example

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "2.0.0",
  "workspaces": ["services/*", "tools/codegen"],
  "exclude": ["services/legacy"],
  "engines": { "node": ">=22", "rust": ">=1.85" },
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"],
  "tasks": {
    "build": { "ignore": ["**/*.test.*"] },
    "test": { "dependsOn": ["build"], "env": ["DATABASE_URL"], "timeout": "10m" }
  },
  "overrides": {
    "web": {
      "dependsOn": ["api"],
      "tasks": {
        "build": { "outputs": ["dist/**", ".astro/**"] },
        "e2e": { "run": "playwright test", "dependsOn": ["build"], "cache": false }
      }
    },
    "tools/codegen": {
      "name": "codegen",
      "driver": "none",
      "tasks": { "build": { "run": "make proto", "outputs": ["gen/**"] } }
    },
    "services/*": { "engines": { "go": ">=1.23" } }
  },
  "settings": { "maxCacheSize": "10GB" }
}
```
