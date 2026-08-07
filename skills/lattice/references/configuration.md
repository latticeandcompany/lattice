# `lattice.json` reference

One file at the repo root drives everything. Every command that reads config
walks up from the current directory to the nearest `lattice.json` and treats
that directory as the repo root. No config found in this directory or any
parent:

```text
Error: no lattice.json found in this directory or any parent; run `lattice init` to create one
```

Every top-level key is optional, so `{}` parses and validates. A key that is not
part of the config, at any level, fails the load:

```text
Error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache
```

The message names the key, the object holding it (`workspaces[1]`, `tasks.build`,
`engines.node`, or `at the top level of lattice.json`), the position, the nearest
valid field within a character or two, and everything accepted there. There is no
way to park an extra key in the file.

## Top level

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `$schema` | `string` | no | none |
| `latticeVersion` | `string` | no | none |
| `workspaces` | array of workspace objects | no | `[]` |
| `engines` | engine map | no | `{}` |
| `globalDependencies` | array of `string` | no | `[]` |
| `globalEnv` | array of `string` | no | `[]` |
| `tasks` | object of `string` → task object | no | `{}` |
| `settings` | settings object | no | `{}` |

`globalDependencies` holds globs relative to the **repo root**, hashed into
every task's cache key. A task's `inputs` are relative to its own workspace and
`tasks` is shared across workspaces, so a file above the workspace has no
`inputs` spelling that means the same thing everywhere; this is where it goes.
`globalEnv` is the same idea for variable names. Both default to empty, and a
shared root file listed in neither is invisible to the cache.

`$schema` is conventionally `".lattice/schema.json"` — a copy of the bundled
schema written next to the config so editors validate and autocomplete it.
Every command that loads config writes that file **only if absent**; an existing
copy is never overwritten. It is the one thing under `.lattice/` meant to be
committed (`cache/`, `toolchains/`, and `bin/` are gitignored). The schema sets
`additionalProperties: false` at every level, matching the parser, so a typo is
underlined as you type and fatal when you run.

`latticeVersion` pins which Lattice release this repo runs. `lattice upgrade`
writes it; every later invocation reads it and hands over to that version unless
the check is suppressed.

## The workspace object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | `string` | yes | — | Unique across the file. |
| `path` | `string` | yes | — | Literal directory relative to the repo root. Never a glob. Must be non-empty. |
| `auto` | `boolean` | no | `true` | Infer the driver, engine, and task commands from the directory's own evidence. `false` disables all inference. |
| `engines` | engine map | no | `{}` | Per-workspace constraints. A key here overrides the same key at the root. |
| `dependsOn` | array of `string` | no | none | Other workspaces, by `name`. |
| `scripts` | object of `string` → `string` | no | `{}` | Task name → exact shell command. Wins over anything inferred. |

`path` is checked for existence as-is:

```text
workspace path 'packages/*' does not point to a directory; workspace paths
are literal directories, not globs
```

Two workspaces cannot share a `name` or resolve to the same directory — both
are errors, not a merge.

With `auto: true` (the default), a task the detected driver has no command for
is skipped silently for that workspace; not every workspace has to run every
task. With `auto: false`, the same situation is fatal:

```text
Error: workspace 'app' is "auto": false but declares no command for task 'build';
add it under this workspace's "scripts" map in lattice.json
```

A workspace's `dependsOn` declares nothing about ordering by itself. It only
takes effect where a task's `dependsOn` uses a `^`-prefixed name.

## The engine map

Name-keyed. Each value is either a bare version-constraint string or an object.

### String form

```json
{ "engines": { "node": ">=20.0.0" } }
```

Valid only for a name Lattice has a built-in version rule for. Every tool in the
driver table qualifies, plus the language toolchains `rust`, `php`, `elixir`, and
`haskell`/`ghc` — see `references/toolchains.md` for the full list. Anything else
is rejected when the config loads, with the object form spelled out in the
message.

### Object form

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `version` | `string` | no | none | Constraint, e.g. `">=13.0.0"`. |
| `versionCmd` | `string` | no | none | Command that prints the tool's version. Required for any name not in the well-known list. |
| `installCmd` | `string` | no | none | Command that installs the toolchain. Its presence is what selects provisioning. Receives `$LATTICE_TOOLCHAIN_DIR` as an environment variable and by literal substitution into the string. |
| `bin` | `string` | no | `"bin"` | Bin directory relative to the install, prepended to the task's `PATH`. |

```json
{
  "engines": {
    "protoc": {
      "version": ">=25.0.0",
      "versionCmd": "protoc --version",
      "installCmd": "curl -fsSL https://example.com/protoc.sh | sh -s -- \"$LATTICE_TOOLCHAIN_DIR\"",
      "bin": "bin"
    }
  }
}
```

A workspace's `engines` map merges with the root's; the workspace wins per key.
See `toolchains.md` for which shape selects which mode and where a provisioned
tool lands.

## The task object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `dependsOn` | array of `string` | no | none | `^task` = that task in each workspace this one depends on. Bare `task` = that task in this same workspace. |
| `inputs` | array of `string` | no | none | Globs whose file contents feed the cache key. |
| `outputs` | array of `string` | no | none | Globs captured as the cached artifact. |
| `ignore` | array of `string` | no | none | Globs subtracted from what `inputs` matched. |
| `env` | array of `string` | no | none | Variable *names* whose resolved values feed the cache key. |
| `persistent` | `boolean` | no | `false` | Never cached, forces raw output for the whole run, must be a graph leaf. Its exit is reported and a non-zero one fails the run. |
| `cache` | `boolean` | no | `true` | `false` opts a non-persistent task out of caching. |
| `timeout` | `string` or integer | no | none | `"90s"`, `"10m"`, `"1h"`, or seconds. On overrun the task's process group gets `SIGTERM`, five seconds, then `SIGKILL`, and the task counts as failed. Ignored on a `persistent` task. Not part of the cache key. |

Keys are task names you choose. Declaration order is preserved and means
nothing — the dependency graph decides execution order. `^task` and bare `task`
are the only two forms; there is no glob or `workspace#task` addressing. An
empty task object `{}` is valid.

Running a name that isn't a key here:

```text
Error: task 'build' is not defined in lattice.json; available tasks: test
```

## Settings

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `maxCacheSize` | `string` or integer | no | none | Cache size bound: an integer plus `B`/`KB`/`MB`/`GB`/`TB` (base 1024, case-insensitive), or a bare integer of bytes. Enforced after every run, and used by `lattice prune` when `--max-size` isn't passed. Unset, the cache grows without limit. |
| `cacheDir` | `string` | no | `".lattice/cache"` | Local cache directory, relative to the repo root. |
| `loquacious` | `boolean` | no | `false` | Equivalent to always passing `-l`. |
| `versionCheck` | `boolean` | no | `true` | `false` disables the `latticeVersion` drift check and handover entirely. |

## Validation summary

| Problem | When caught | Message |
| --- | --- | --- |
| No `lattice.json` in this or any parent directory | before parsing | `no lattice.json found in this directory or any parent; run \`lattice init\` to create one` |
| Malformed JSON | parsing | `failed to parse lattice.json`, with the JSON error and position |
| Workspace missing `name` or `path` | parsing | `missing field \`name\`` / `missing field \`path\``, with line and column |
| A key that is not part of the config, at any level | parsing | `unknown field \`<key>\` in <path>`, with position, the nearest valid field, and the fields accepted there |
| Engine value neither a string nor a valid object | parsing | `invalid type: <what was written>, expected a version constraint string or an engine object` |
| Workspace `path` empty or whitespace-only | validation | `workspace '<name>' has an empty path` |
| Workspace `path` absolute, or escaping the repo root with `..` | validation | `workspace '<name>' has a path '<path>' that points outside the repo root` |
| Two workspaces with the same `name` | validation | `duplicate workspace name '<name>': workspace names must be unique` |
| Workspace `dependsOn` naming an undeclared workspace | validation | `workspace '<name>' depends on '<dep>', which is not a declared workspace`, with the nearest name and the full list |
| Task `dependsOn` naming a task not in `tasks` (`^` stripped first) | validation | `task '<name>' depends on '<dep>', but '<dep>' is not defined in \`tasks\``, with the nearest name and the full list |
| String-form engine that isn't well-known | validation | names the engine and shows the object form with `versionCmd` |
| Task name not present in `tasks` | after config loads | `task '<name>' is not defined in lattice.json; available tasks: ...` |
| `lattice prune` with no size limit anywhere | after config loads | `no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)` |
| Cycle through `dependsOn` | graph construction | `cycle detected in task dependency graph` |
| Something depends on a persistent task | graph construction | `persistent task '<task>' in workspace '<ws>' cannot be depended on by other tasks` |

## Complete example

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=20.0.0" } },
    { "name": "api", "path": "services/api", "dependsOn": ["shared"] },
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
  "engines": { "rust": ">=1.75.0" },
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
    "lint": { "inputs": ["src/**/*"] },
    "dev": { "persistent": true },
    "clean": {}
  },
  "settings": { "maxCacheSize": "10GB" }
}
```
