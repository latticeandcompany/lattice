---
title: Configuration
description: Exhaustive field reference for lattice.json.
group: Reference
order: 1
---

# Configuration

Everything Lattice does in a repo is driven by one file: `lattice.json` at the
repo root. This page enumerates every field it accepts — type, whether it's
required, its default, and what it's for. For what these fields mean in
practice, see [Workspaces](/lattice/docs/workspaces),
[Task graph](/lattice/docs/task-graph), [Caching](/lattice/docs/caching), and
[Engines and provisioning](/lattice/docs/engines).

## Where the file lives

`lattice.json` lives at the repo root. Every command that reads config walks up
from the current directory looking for the nearest `lattice.json` and treats
that directory as the repo root — so subcommands work from any workspace
subdirectory. If no `lattice.json` is found in the current directory or any
parent, Lattice fails immediately:

```text
Error: no lattice.json found in this directory or any parent; run `lattice init` to create one
```

An empty `lattice.json` — literally `{}` — is valid: every top-level key is
optional. Running a task against it fails only because no tasks are declared:

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

Every command that loads config first calls `ensure_schema`, which writes
`.lattice/schema.json` **only if it is absent** — an existing copy (including
one you've pinned or hand-edited) is never overwritten. `lattice init` writes
it explicitly as part of scaffolding. Commit `.lattice/schema.json`; it isn't
one of the gitignored `.lattice/` artifacts (`cache/`, `toolchains/`, `bin/`
are).

The bundled schema is stricter than Lattice's own parser in one respect: the
schema sets `additionalProperties: false` at every level, so an editor flags an
unknown key (a stray `projects`, a typo'd field) immediately. The parser itself
silently ignores unknown keys at parse time — they simply have no effect. Treat
editor validation against `.lattice/schema.json` as the authoritative check for
typos; don't rely on `lattice run` to catch them.

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
form; the schema and the parser both reject a bare string array or a `glob`
key on a workspace entry.

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
well-known engines Lattice has a built-in version rule for:

| `node` | `deno` | `bun` | `pnpm` | `yarn` | `npm` |
| --- | --- | --- | --- | --- | --- |
| `rust` | `cargo` | `go` | `python` | `python3` | `ruby` |
| `bundler` | `java` | `gradle` | `maven` | `dotnet` | |

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
| `inputs` | array of `string` | no | none | File globs hashed to compute the cache key. |
| `outputs` | array of `string` | no | none | File globs captured as the cached artifact. |
| `ignore` | array of `string` | no | none | File globs excluded from input hashing. |
| `env` | array of `string` | no | none | Environment variable **names** whose resolved values feed the cache key. |
| `persistent` | `boolean` | no | `false` | Long-running task (e.g. a dev server). Never cached regardless of `cache`. See [Persistent tasks](/lattice/docs/persistent-tasks). |
| `cache` | `boolean` | no | `true` | Set `false` to opt a non-persistent task out of caching entirely. |

An empty task object, `{}`, is valid — a task with no declared dependencies,
inputs, or outputs.

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
| `maxCacheSize` | `string` | no | none | Upper bound on the local cache size. Human byte size: an integer plus `B`/`KB`/`MB`/`GB`/`TB` (base 1024, case-insensitive), or a bare integer of bytes. Used by `lattice prune` when `--max-size` isn't passed. |
| `logging` | `string` | no | none | Logging verbosity mode. |
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
| An engine value that is neither a string nor a valid object | parsing | `data did not match any variant of untagged enum EngineSpec` |
| A workspace `path` that is empty or whitespace-only | validation | `workspace '<name>' has an empty path` |
| Two workspaces with the same `name` | validation | `duplicate workspace name '<name>': workspace names must be unique` |
| A string-form engine whose name isn't well-known | validation | names the engine and suggests the object form with `versionCmd` |
| A task name passed to `lattice run` not present in `tasks` | after config loads | `task '<name>' is not defined in lattice.json; available tasks: ...` |
| `lattice prune` with no size limit anywhere | after config loads | `no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)` |

An unrecognized top-level or nested key (for example a leftover `projects`
map, or a `glob` key on a workspace) is accepted by the parser and silently
ignored — it has no effect on any command. The bundled JSON Schema rejects the
same input, which is why editor validation against `.lattice/schema.json`
catches typos the CLI itself will not.

## Complete example

A repo with a Node app, a Rust service, and a Python library wired for tasks,
selective caching, and pinned tool versions:

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
      "env": ["DATABASE_URL"]
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
