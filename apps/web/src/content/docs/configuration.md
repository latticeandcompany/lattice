---
title: Configuration
description: Exhaustive field reference for lattice.json.
group: Reference
order: 1
---

# Configuration

One file drives everything Lattice does in a repo: `lattice.json` at the repo
root. Every field it accepts is below, with its type, whether it's required,
and its default. For what these fields mean in practice, see
[Workspaces](/lattice/docs/workspaces),
[Task graph](/lattice/docs/task-graph), [Caching](/lattice/docs/caching), and
[Engines and provisioning](/lattice/docs/engines).

## Where the file lives

Every command that reads config walks up from the current directory to the
nearest `lattice.json` and treats that directory as the repo root, so
subcommands work from any workspace subdirectory. With no `lattice.json` in the
current directory or any parent, Lattice fails immediately:

```text
Error: no lattice.json found in this directory or any parent; run `lattice init` to create one
```

Every top-level key is optional, so an empty `lattice.json` is valid. Running a
task against it fails only because no tasks are declared:

```json
{}
```

## The `$schema` field and editor validation

```json
{ "$schema": ".lattice/schema.json" }
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `$schema` | `string` | no | none |

`$schema` is a plain string reference, conventionally `.lattice/schema.json` —
a copy of Lattice's bundled JSON Schema written next to your config so editors
with JSON Schema support (VS Code, JetBrains) validate and autocomplete
`lattice.json` as you type.

Every command that loads config writes `.lattice/schema.json` **only if it is
absent**; an existing copy, including one you've pinned or hand-edited, is
never overwritten. `lattice init` writes it explicitly as part of scaffolding.
Commit `.lattice/schema.json` — it isn't one of the gitignored `.lattice/`
artifacts (`cache/`, `toolchains/`, `bin/` are).

## Unknown keys

The bundled schema sets `additionalProperties: false` at every level, and the
parser holds the same line: a key Lattice does not recognize fails the load
before any task is scheduled. Your editor underlines it as you type, and
`lattice run` refuses the file.

```text
Error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache
```

The message names the key, the object it sits in, its position in the file, and
the closest field within a couple of characters. A workspace entry is indexed —
`workspaces[1]` — so the right one of several gets looked at.

Writing `output` for `outputs` is the case this exists for. The task would
declare nothing to capture, so a cache hit would restore no files. `input` for
`inputs` would silently widen the key to the whole workspace, turning a narrow,
fast task into one that re-runs on any change in its directory.

There is no way to keep extra keys in the file. A `lattice.json` upgraded from
an earlier release, or one holding a note under a key of your own, has to drop
them.

## `latticeVersion`

```json
{ "latticeVersion": "1.0.0-beta-2" }
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `latticeVersion` | `string` | no | none |

Pins the version of Lattice this repo runs, so every contributor and every CI
job runs the same build. See [Upgrading](/lattice/docs/upgrading) for how the
version-drift check uses this field and `settings.versionCheck`.

## `workspaces`

```json
{
  "workspaces": [
    { "name": "web", "path": "apps/web" },
    { "name": "api", "path": "apps/api", "dependsOn": ["web"] }
  ]
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `workspaces` | array of workspace objects | no | `[]` |

Each entry is one workspace — a single project directory. There is no glob
form. A bare string array (`"workspaces": ["apps/*"]`) fails to parse, and so
does a `glob` key on a workspace entry.

### The workspace object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | `string` | yes | — | Workspace name. Must be unique across the file. |
| `path` | `string` | yes | — | Literal directory path, relative to the repo root. Never a glob. Must be non-empty. |
| `auto` | `boolean` | no | `true` | When true, infer the engine and task commands from the workspace's native manifest. `false` disables all inference — declare `scripts` and `engines` yourself. |
| `engines` | engine map | no | `{}` | Per-workspace toolchain constraints. A key here overrides the same key at the root. |
| `dependsOn` | array of `string` | no | none | Other workspaces, by name, this one depends on. |
| `scripts` | object of `string` → `string` | no | `{}` | Explicit script-name → command overrides. |

A workspace without `name` or without `path` is rejected at parse time:

```text
Error: failed to parse lattice.json

Caused by:
    missing field `name` at line 2 column 32
```

A workspace with an empty (or whitespace-only) `path`, or two workspaces
sharing a `name`, are rejected by validation after parsing:

```text
Error: workspace 'empty-path' has an empty path
Error: duplicate workspace name 'dup': workspace names must be unique
```

Minimal validating example:

```json
{
  "workspaces": [
    {
      "name": "utils",
      "path": "libs/utils",
      "auto": false,
      "scripts": { "build": "python3 setup.py build" }
    }
  ]
}
```

## `engines`

```json
{ "engines": { "node": ">=20.0.0", "rust": ">=1.75.0" } }
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `engines` | engine map | no | `{}` |

An engine map is a name-keyed object. Each value is either the **string form**
(a bare version constraint) or the **object form** (an explicit spec). Root
`engines` are defaults for every workspace; a workspace's own `engines` entry
overrides the root entry of the same name — see
[Engines and provisioning](/lattice/docs/engines) for the full mode gradient
this drives.

### String form

```json
{ "engines": { "node": ">=20.0.0" } }
```

A bare version-constraint string. The engine name must be one of the
well-known engines Lattice has a built-in version rule for — every built-in
driver, plus the language toolchains those drivers sit on top of:

| `node` | `deno` | `bun` | `pnpm` | `yarn` | `npm` |
| --- | --- | --- | --- | --- | --- |
| `rust` | `cargo` | `go` | `python` | `python3` | `pip` |
| `uv` | `poetry` | `pdm` | `pipenv` | `ruby` | `bundler` |
| `rake` | `java` | `kotlin` | `gradle` | `maven` | `dotnet` |
| `nuget` | `swift` | `pod` | `php` | `composer` | `elixir` |
| `mix` | `dart` | `haskell` | `ghc` | `stack` | `cabal` |
| `just` | `task` | `turbo` | `nx` | | |

Each name's version command is listed on
[Toolchains](/lattice/docs/toolchains#well-known-engines).

A string-form engine outside this list fails validation:

```text
Error: engine 'alpes' in root uses the string (version-only) form, but 'alpes'
is not a well-known engine Lattice can version-check on its own. Use the
object form with an explicit `versionCmd`, e.g. "alpes": { "version":
">=1.0.0", "versionCmd": "alpes --version" }
```

### Object form

```json
{
  "engines": {
    "alpes": {
      "version": ">=2.6.7",
      "versionCmd": "alp --version",
      "installCmd": "curl https://example.com/alp.sh | sh",
      "bin": "bin"
    }
  }
}
```

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `version` | `string` | no | none | Version constraint, e.g. `">=13.0.0"`. |
| `versionCmd` | `string` | no | none | Command that prints the tool's version. Required for any engine name not in the well-known list. |
| `installCmd` | `string` | no | none | Command that installs the toolchain. Its presence is what selects provisioning mode. Receives `$LATTICE_TOOLCHAIN_DIR` both as an environment variable and by literal substitution into the command string. |
| `bin` | `string` | no | `"bin"` | Bin directory relative to the toolchain install, prepended to the task's `PATH`. |

The object form works for any engine name, well-known or not, as long as an
unknown name supplies `versionCmd`. Which fields are present selects the mode —
host `PATH`, validate-only, or provision — as described in
[Engines and provisioning](/lattice/docs/engines).

## `globalDependencies`

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**", ".env"]
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `globalDependencies` | array of `string` | no | `[]` |

Globs relative to the **repo root**, hashed into the cache key of every task. A
task's `inputs` are relative to its own workspace, so a file above the workspace
cannot be named there in a way that means the same thing everywhere; this is
where those files go. Editing anything matched here makes every task miss, so
list only what genuinely crosses workspace boundaries. See
[Caching](/lattice/docs/caching#files-shared-across-workspaces).

## `globalEnv`

```json
{
  "globalEnv": ["NODE_ENV", "CI"]
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `globalEnv` | array of `string` | no | `[]` |

Environment variable **names** whose resolved values feed the cache key of every
task, for variables that change what any build produces. A task's own `env` list
applies on top of this one. Unlike `env`, these names are not exported into task
processes — they are already in the environment Lattice inherited.

## `tasks`

```json
{
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "package.json"],
      "outputs": ["dist/**"]
    }
  }
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `tasks` | object of `string` → task object | no | `{}` |

Keys are task names; order of declaration is preserved but does not affect
execution order (that's the dependency graph — see
[Task graph](/lattice/docs/task-graph)). Running a task name that isn't a key
in this map fails immediately, listing what is defined:

```text
Error: task 'build' is not defined in lattice.json; available tasks: test
```

### The task object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `dependsOn` | array of `string` | no | none | Task dependencies. `^task` means the same task name in this task's dependency workspaces; a bare `task` means that task in the same workspace. |
| `inputs` | array of `string` | no | the whole workspace | File globs hashed to compute the cache key. Omitted, every file in the workspace is hashed except what `.gitignore` excludes and what this task's own `outputs` match. |
| `outputs` | array of `string` | no | none | File globs captured as the cached artifact. Never hashed as input. |
| `ignore` | array of `string` | no | none | File globs excluded from input hashing. |
| `env` | array of `string` | no | none | Environment variable **names** whose resolved values feed the cache key. |
| `persistent` | `boolean` | no | `false` | Long-running task (e.g. a dev server). Never cached regardless of `cache`. See [Persistent tasks](/lattice/docs/persistent-tasks). |
| `cache` | `boolean` | no | `true` | Set `false` to opt a non-persistent task out of caching entirely. |
| `timeout` | `string` or `integer` | no | none | How long the task may run before it is stopped and counted as failed. `"90s"`, `"10m"`, `"1h"`, or a bare number of seconds. Ignored on a `persistent` task. |

An empty task object, `{}`, is valid — a task with no declared dependencies,
inputs, or outputs. It still caches: its key covers the whole workspace, so it
re-runs whenever anything in that directory changes.

```json
{
  "tasks": {
    "dev": { "persistent": true, "cache": false },
    "clean": {}
  }
}
```

## `settings`

```json
{
  "settings": {
    "maxCacheSize": "10GB",
    "cacheDir": ".lattice/cache",
    "loquacious": false,
    "versionCheck": true
  }
}
```

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `maxCacheSize` | `string` | no | none | Upper bound on the local cache size. Human byte size: an integer plus `B`/`KB`/`MB`/`GB`/`TB` (base 1024, case-insensitive), or a bare integer of bytes. Enforced after every run, and used by `lattice prune` when `--max-size` isn't passed. Unset, the cache grows without limit. |
| `cacheDir` | `string` | no | `".lattice/cache"` | Directory for the local cache, relative to the repo root. |
| `loquacious` | `boolean` | no | `false` | Equivalent to always passing `-l`/`--loquacious`: forces raw, unbuffered output. |
| `versionCheck` | `boolean` | no | `true` | When true, compare the running binary against `latticeVersion` and nag on drift. `false` disables the check entirely — see [Upgrading](/lattice/docs/upgrading). |

`maxCacheSize` accepts either unit form:

```json
{ "settings": { "maxCacheSize": "512MB" } }
```

```json
{ "settings": { "maxCacheSize": 536870912 } }
```

If `settings.maxCacheSize` is unset and `lattice prune` is run without
`--max-size`, it fails:

```text
Error: no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)
```

## Validation summary

| Problem | When caught | Error |
| --- | --- | --- |
| No `lattice.json` in this or any parent directory | before parsing | `no lattice.json found in this directory or any parent; run \`lattice init\` to create one` |
| Malformed JSON | parsing | `failed to parse lattice.json` (with the underlying JSON error and position) |
| A workspace object missing `name` or `path` | parsing | `missing field \`name\`` / `missing field \`path\`` (with line/column) |
| A key Lattice doesn't recognize, at any level | parsing | `unknown field \`<key>\` in <path>` (with position, the nearest valid field, and the fields accepted there) |
| An engine value that is neither a string nor a valid object | parsing | `invalid type: <what was written>, expected a version constraint string or an engine object` |
| A workspace `path` that is empty or whitespace-only | validation | `workspace '<name>' has an empty path` |
| A workspace `path` that is absolute or escapes the repo root | validation | `workspace '<name>' has a path '<path>' that points outside the repo root` |
| Two workspaces with the same `name` | validation | `duplicate workspace name '<name>': workspace names must be unique` |
| A workspace `dependsOn` naming a workspace that isn't declared | validation | `workspace '<name>' depends on '<dep>', which is not a declared workspace` (with the nearest name and the full list) |
| A task `dependsOn` naming a task that isn't in `tasks` | validation | `task '<name>' depends on '<dep>', but '<dep>' is not defined in \`tasks\`` (with the nearest name and the full list) |
| A string-form engine whose name isn't well-known | validation | names the engine and suggests the object form with `versionCmd` |
| A task name passed to `lattice run` not present in `tasks` | after config loads | `task '<name>' is not defined in lattice.json; available tasks: ...` |
| `lattice prune` with no size limit anywhere | after config loads | `no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)` |

Everything under "parsing" is raised before Lattice looks at a single workspace
directory, so nothing has run when you see it.

## Complete example

A repo with a Node app, a Rust service, and a Python library:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=20.0.0" } },
    {
      "name": "api",
      "path": "services/api",
      "dependsOn": ["shared"]
    },
    {
      "name": "shared",
      "path": "libs/shared",
      "auto": false,
      "scripts": {
        "build": "python3 -m build",
        "test": "python3 -m pytest",
        "lint": "python3 -m ruff check ."
      }
    }
  ],
  "engines": {
    "rust": ">=1.75.0",
    "protoc": {
      "version": ">=25.0.0",
      "versionCmd": "protoc --version",
      "installCmd": "curl -fsSL https://example.com/protoc.sh | sh -s -- \"$LATTICE_TOOLCHAIN_DIR\"",
      "bin": "bin"
    }
  },
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "Cargo.toml", "package.json"],
      "ignore": ["**/*.test.*"],
      "outputs": ["dist/**", "target/release/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["src/**/*", "tests/**/*"],
      "env": ["DATABASE_URL"],
      "timeout": "10m"
    },
    "lint": {
      "inputs": ["src/**/*"]
    },
    "dev": {
      "persistent": true,
      "cache": false
    },
    "clean": {}
  },
  "settings": {
    "maxCacheSize": "10GB",
    "versionCheck": true
  }
}
```
