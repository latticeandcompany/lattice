# `lattice.json` reference

One optional file at the repo root. Every command that reads config walks up
from the current directory to the nearest `lattice.json` and treats that
directory as the repo root. With none in this directory or any parent:

```text
error: no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

Every key is optional, so `{}` is a complete config for a repo whose own
workspace files already name its workspaces. What the file holds is only what
discovery cannot work out: extra workspace directories, exclusions, task
settings, per-workspace overrides, engine pins, and cache settings.

A key the config does not define, at any level, fails the load:

```text
error: unknown field `output` in tasks.build (lattice.json line 1, column 27)
Did you mean `outputs`?
Fields accepted here: run, dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

The message names the key, the object holding it (`tasks.build`,
`overrides.web`, `engines.node`, or `at the top level of lattice.json`), the
position, the nearest valid field within one or two edits (a swap of two
neighbouring letters counts as one), and every field accepted there.

A key written twice in a map fails too. There is no last-wins behavior in
`tasks`, `engines`, `overrides`, an override's `engines`, or an override's
`tasks`:

```text
error: duplicate key `build` in tasks (lattice.json line 1, column 33)
Keep one of them: the second replaces the first, so only the last would take effect
```

The position is where the parser finished the object, so it points at the
closing brace rather than the repeat. A field repeated on a fixed object — a
top-level key, a key inside `settings`, an override, or one task — is reported
by the parser in its own words:

```text
error: failed to parse lattice.json
  duplicate field `name` at line 1 column 36
```

A 1.x config, one whose `workspaces` holds objects, is refused before anything
else is checked:

```text
error: lattice.json is written for Lattice 1.x, which listed every workspace as an object with a name and a path. Run `lattice migrate` to rewrite it for this version
```

## Top level

| Field | Type | Default |
| --- | --- | --- |
| `$schema` | `string` | none |
| `latticeVersion` | `string` | none |
| `workspaces` | array of `string` | `[]` |
| `exclude` | array of `string` | `[]` |
| `engines` | engine map | `{}` |
| `globalDependencies` | array of `string` | `[]` |
| `globalEnv` | array of `string` | `[]` |
| `tasks` | object of task name to task object | `{}` |
| `overrides` | object of key to override object | `{}` |
| `settings` | settings object | `{}` |

There is no `projects`, `pipeline`, `include` or `extends` key.

`$schema` is conventionally `".lattice/schema.json"`, a copy of the bundled
schema written next to the config so editors validate and autocomplete it.
Every command that loads the config writes that file when it is missing or
differs from the binary's own copy (line endings aside), and `lattice init` and
`lattice migrate` write it too. Commit it. The rest of `.lattice/` — `cache/`,
`toolchains/`, `bin/`, `setup/` — is per-machine and gitignored.

`latticeVersion` pins which Lattice release the repo runs; `lattice upgrade`
writes it. A binary Lattice installed under `.lattice/bin` hands every
invocation over to the pinned version, installing it if needed, and refuses a
value that does not parse as a version: `lattice.json pins \`latticeVersion\`
as "<value>", which is not a version. Write it like 0.2.0, or run \`lattice
upgrade <version>\` to set it`. Any other binary (npm, `cargo install`, a
source build) runs as it is and prints a one-line nag in an interactive
session.

`globalDependencies` holds globs relative to the **repo root** whose contents
are hashed into every task's cache key: the only way to cover a file above a
workspace, such as a base `tsconfig.json` or a shared schema directory.
`globalEnv` holds variable names hashed into every key, resolved from the
ambient environment and not re-exported (the task inherits it anyway).

## `workspaces` and `exclude`

`workspaces` adds directories to the ones the repo root's own workspace files
name (`package.json` `workspaces`, `pnpm-workspace.yaml`, Cargo `[workspace]`,
`go.work`, uv `[tool.uv.workspace]`, deno `workspace`). Each entry is a
directory or a glob relative to the root: `"services/api"`, `"apps/*"`,
`"packages/**"`. `*` does not cross a `/`. `"."` is the root itself.

`exclude` is globs of workspace paths removed from everything discovery found,
native or listed.

- An entry with no glob character is looked up directly; if it is not a
  directory, the load fails: ``error: `workspaces` entry 'tolls/*' matches no
  directory. Check the spelling, or remove the entry`` (the same wording for a
  glob that matches nothing).
- A glob walks the tree, skipping hidden and gitignored directories and
  `node_modules`, `target`, `dist`, `build`, `out`, `vendor`, `venv` and
  `__pycache__`.
- A directory an entry matches that holds nothing Lattice recognizes is not a
  workspace and says nothing, unless an override writes out a command for it.
- A directory listed here is one workspace whatever it contains: a nested
  monorepo, a Cargo workspace, a Gradle build with subprojects. Only the repo
  root's workspace files are expanded into members.
- With no native members and no `workspaces`, the root is the one workspace.
  With either, the root is a workspace only if listed as `"."`, or if it is a
  Cargo package or uv project as well as a workspace root.

Entries must stay inside the repo:

```text
error: `workspaces` entry '../x' points outside the repo root. Every workspace has to be inside the repo
```

An absolute or drive-prefixed entry, whitespace around a directory name, and an
invalid glob (``error: `workspaces` entry 'a/[' is not a valid glob``) are
rejected the same way, in `exclude` too. A directory that resolves outside the
repo through a symlink is rejected at discovery.

## Workspace names

A workspace takes the name its manifest gives it: `package.json` `name` (scope
included), else Cargo `[package] name`, else pyproject `[project] name`, else
`composer.json` `name`, else its directory's name (the root's directory name for
the root). Two that derive the same name are both named by their relative path.
Two named the same on purpose, through `overrides`, fail:

```text
error: two workspaces are named 'web'. Give each a different `name` in `overrides`
```

## `overrides`

Keys are **workspace names**, or **globs over workspace paths** when the key is
`"."` or contains a `/` or a glob character (`*`, `?`, `[`, `{`). A top-level
directory is matched by path as `"./tools"`. A name key matches the discovered
name, or a name a path override gave the workspace.

| Field | Type | Description |
| --- | --- | --- |
| `name` | `string` | The workspace's name, in place of its manifest's. Non-empty |
| `driver` | `string` | The tool that runs its tasks: any driver in `toolchains.md`, or `"none"` for a workspace whose every task is written out under `tasks` |
| `engines` | engine map | Pins for these workspaces, over the root `engines` per name |
| `dependsOn` | array of `string` | Workspace names this one depends on beyond what its manifest says |
| `tasks` | object of task name to command string or task object | Commands and settings for these workspaces |

Several keys can match one workspace. They apply in the order written: `name`
and `driver` last-wins, `engines` merged per name, `dependsOn` combined without
repeats, and each task layered field by field.

A task value is a command string (`"build": "vite build"`) or a task object; a
task object's `run` is the command. A task with a `run` replaces the command
discovery found, or adds a task the workspace did not have. A task object with
no `run` changes only the settings of a task the workspace already has, and
does nothing in a workspace that lacks it.

Errors:

```text
error: `overrides` key 'weeb' names no workspace
Did you mean `web`?
Workspaces: api, web
```

```text
error: workspace 'gen' names driver 'pnmp', which Lattice does not know
Did you mean `pnpm`?
Use one of node, deno, bun, pnpm, yarn, npm, cargo, go, uv, poetry, pdm, pipenv, pip, python, bundler, rake, ruby, gradle, maven, java, kotlin, dotnet, nuget, pod, swift, composer, mix, dart, stack, cabal, just, task, turbo, nx, or "none" and write each task's command under `tasks`
```

```text
error: workspace 'web' depends on 'apii', which is not a workspace
Did you mean `api`?
Workspaces: api, web
```

A path key that matches nothing says `matches no workspace path`. A workspace
that lists itself in `dependsOn` fails with `workspace '<name>' lists itself in
\`dependsOn\``. An empty `name` fails with ``\`overrides.<key>.name\` is
empty``.

## The engine map

Name-keyed. Each value is a bare version-constraint string or an object. Root
`engines` apply to every workspace; override `engines` win per name.

### String form

```json
{ "engines": { "node": ">=22" } }
```

Valid only for the 40 well-known engine names in `toolchains.md`:

```text
error: engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
```

The scope reads `root` or `overrides.<key>`.

### Object form

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `version` | `string` | none | Constraint, such as `">=13.0.0"`, `"^1.75"`, or a bare `"22.11.0"` (read as `^22.11.0`) |
| `versionCmd` | `string` | the built-in rule | Command that prints the tool's version |
| `installCmd` | `string` | none | Command that installs the tool. Its presence selects provisioning |
| `bin` | `string` | `"bin"` | Directory inside the install to put on `PATH`. Relative, non-empty, inside the install |

```json
{
  "engines": {
    "protoc": {
      "version": ">=25.0.0",
      "versionCmd": "protoc --version",
      "installCmd": "curl -fsSL https://example.com/protoc.sh | sh -s -- \"$LATTICE_TOOLCHAIN_DIR\""
    }
  }
}
```

A value that is neither form fails while parsing (`invalid type: integer
\`20\`, expected a version constraint string or an engine object`). A `bin` of
`"/usr/bin"` or `"../../.."` fails at load:

```text
error: engine 'node' in overrides.web has a `bin` of '/usr/bin', which is not relative to the toolchain install. Write `bin` as a path inside the toolchain directory, like "bin"
```

A `version` on a name with no built-in rule and no `versionCmd` passes the load
and fails the first run with `engine '<name>' has a version constraint but no
way to check the installed version`.

## The task object

Used in the root `tasks` map (how a task behaves in every workspace that has
it) and in an override's `tasks` (for the workspaces it matches). A task does
not need an entry in either to run.

| Field | Type | Description |
| --- | --- | --- |
| `run` | `string` | The command. Overrides only; in the root map it fails the load |
| `dependsOn` | array of `string` | `^task` is that task in each workspace this one depends on; bare `task` is that task in this workspace |
| `inputs` | array of `string` | Workspace-relative globs hashed into the cache key. Without it, the whole workspace minus gitignored files |
| `outputs` | array of `string` | Workspace-relative globs archived on success and restored on a hit. Replaces the tool's default |
| `ignore` | array of `string` | Globs subtracted from what `inputs` matched |
| `env` | array of `string` | Variable names whose values are hashed; set ones are exported to the task |
| `persistent` | `boolean` | A long-running task: never cached, nothing may depend on it, forces the plain stream for the run |
| `cache` | `boolean` | `false` always runs the task |
| `timeout` | `string` or integer | `"90s"`, `"10m"`, `"1h"`, or whole seconds, up to 365 days. On overrun the process group gets `SIGTERM`, five seconds, then `SIGKILL`, and the task fails. Ignored on a persistent task |

Settings come from four layers, each replacing only the fields it sets:
Lattice's defaults for the name (`build`: `dependsOn: ["^build"]`; `dev`,
`start`, `serve`, `watch`: `persistent: true`; `clean`, `deploy`, `publish`,
`release`: `cache: false`), the tool's default `outputs` for a `build` the
workspace's own files define, the root `tasks` entry, then the override. Lists
replace; they never append.

```text
error: `tasks.build` has a `run`, but the root `tasks` map says how a task behaves in every workspace, not what it runs. Put the command under the workspace it belongs to: "overrides": { "<workspace>": { "tasks": { "build": "<command>" } } }
```

`timeout` takes `ms`, `s`, `sec`, `secs`, `m`, `min`, `mins`, `h`, `hr`,
`hrs`, case-insensitively, or a whole number of seconds. Under a second rounds
up to one; zero and negatives are rejected. Over 365 days is rejected
(`duration '99999h' is longer than the maximum of 365 days`), and a fractional
number (`1.5`) is rejected rather than rounded; write `"90s"`.

`dependsOn` checks:

- A root task naming itself fails the load: `task 'build' lists itself in
  \`dependsOn\``.
- A name no workspace has fails the plan: `task 'build' depends on 'codegne',
  but no workspace has a 'codegne' task`, with the nearest name and the full
  task list.

## Settings

| Field | Type | Default | Description |
| --- | --- | --- | --- |
| `maxCacheSize` | `string` | none | An integer plus `B`, `KB`, `MB`, `GB` or `TB` (base 1024), or bare bytes, as a string. Enforced after every run that writes, and used by `lattice prune` without `--max-size` |
| `cacheDir` | `string` | `".lattice/cache"` | Cache directory relative to the root. Inside the repo, not the root itself |
| `loquacious` | `boolean` | `false` | Always verbose, as if `-v` were passed |
| `versionCheck` | `boolean` | `true` | `false` disables the `latticeVersion` handover and the nag |

`maxCacheSize` has to be a JSON string (`invalid type: integer \`1048576\`,
expected a string`). An unknown unit fails with the units listed. `cacheDir`
fails if empty, absolute, outside the repo, or the root:

```text
error: `settings.cacheDir` is '.', which is the repo root itself. Point it at a directory of its own, like ".lattice/cache" — `lattice prune` deletes cache archives and partial writes in whatever directory it names
```

## When each check runs

| Stage | What fails there |
| --- | --- |
| Finding the root | No `lattice.json` here or above |
| Parsing | 1.x format, malformed JSON, unknown keys, duplicate keys, wrong types, bad durations and sizes |
| Validation | String-form engines not well-known, `bin`, `cacheDir`, `workspaces`/`exclude` entries outside the repo or with stray whitespace, `run` in the root map, a root task depending on itself, an empty override `name` |
| Discovery | A `workspaces` entry matching nothing, an invalid glob, an override key matching nothing, an unknown `driver`, a detection halt, a `dependsOn` naming no workspace, a name clash, a workspace outside the repo through a symlink |
| Planning | A task no workspace has, a `dependsOn` naming no task, a dependency on a persistent task, a cycle |
| First run | Engine checks and installs |

Nothing runs when any of these fires.

## Complete example

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.1.1",
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
