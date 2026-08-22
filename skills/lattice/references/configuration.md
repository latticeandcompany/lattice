# `lattice.json` reference

One file at the repo root drives everything. Every command that reads config
walks up from the current directory to the nearest `lattice.json` and treats
that directory as the repo root. With no config in this directory or any parent:

```text
Error: no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

Every top-level key is optional, so `{}` parses and validates.

A key the config does not define, at any level, fails the load. There is no way
to park an extra key in the file.

```text
Error: unknown field `output` in tasks.build (lattice.json line 1, column 32)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

The message names the key, the object holding it, the position, the nearest
valid field when one is within one or two edits, and every field accepted there.
The container reads the way it does in the file: `tasks.build`,
`workspaces[1]`, `engines.node`, or `at the top level of lattice.json`.

## Top level

Eight keys.

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `$schema` | `string` | no | none |
| `latticeVersion` | `string` | no | none |
| `workspaces` | array of workspace objects | no | `[]` |
| `engines` | engine map | no | `{}` |
| `globalDependencies` | array of `string` | no | `[]` |
| `globalEnv` | array of `string` | no | `[]` |
| `tasks` | object of `string` to task object | no | `{}` |
| `settings` | settings object | no | `{}` |

There is no `projects` key, no `pipeline` key, and no `include` or `extends`.

`globalDependencies` holds globs relative to the **repo root**. Their contents
are hashed into every task's cache key. A task's `inputs` are relative to its own
workspace and the `tasks` map is shared across workspaces, so a file above a
workspace has no `inputs` spelling that means the same thing everywhere. This is
where it goes. `globalEnv` is the same idea for variable names: hashed into every
key, resolved from the ambient environment, and not re-exported to the task,
which already inherits it.

`$schema` is conventionally `".lattice/schema.json"`, a copy of the bundled
schema written next to the config so editors validate and autocomplete it. Every
command that loads config writes that file **only when it is absent**. An
existing copy is never overwritten, even by a newer release. `lattice init`
overwrites it unconditionally. The schema sets `additionalProperties: false` at
every level, matching the parser, so a typo is underlined as you type and fatal
when you run. It is the one thing under `.lattice/` meant to be committed;
`cache/`, `toolchains/`, and `bin/` are gitignored.

`latticeVersion` pins which Lattice release this repo runs. `lattice upgrade`
writes it. A binary that Lattice itself installed under `.lattice/bin` reads the
pin on every invocation, installs the pinned version if needed, and hands the
invocation over to it. A binary from anywhere else is never replaced: it prints
a one-line advisory nag instead, and only in an interactive session.

## The workspace object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | `string` | yes | none | Unique across the file. |
| `path` | `string` | yes | none | Literal directory relative to the repo root. Never a glob. |
| `auto` | `boolean` | no | `true` | Infer the driver and each task's command from the directory's own evidence. `false` disables all inference. |
| `engines` | engine map | no | `{}` | Per-workspace constraints. A key here overrides the same key at the root. |
| `dependsOn` | array of `string` | no | none | Other workspaces, by `name`. |
| `scripts` | object of `string` to `string` | no | `{}` | Task name to exact shell command. Wins over anything inferred. |

`path` is a literal directory. `"apps/*"` matches nothing and fails:

```text
Error: workspace path 'apps/*' does not point to a directory. A workspace path is one literal directory, not a glob
```

There is no `glob` field and no way to expand one workspace entry into several.
Declare each directory.

Two workspaces cannot share a `name`, and two cannot resolve to the same
directory. Both are errors, not a merge.

`dependsOn` on a workspace declares nothing about ordering by itself. It takes
effect only where some task's `dependsOn` uses a `^`-prefixed name.

With `auto: true`, a task the resolved driver has no command for is skipped
silently for that workspace. Not every workspace has to run every task. With
`auto: false`, the same situation is fatal:

```text
Error: workspace 'app' has "auto": false and declares no command for task 'dev'. Add the command under this workspace's "scripts" map in lattice.json
```

Under `--filter`, this check covers the workspaces the pattern matched. A
workspace pulled in only as a dependency is asked only for the tasks its
dependents need.

## The engine map

Name-keyed. Each value is either a bare version-constraint string or an object.

### String form

```json
{ "engines": { "node": ">=20.0.0" } }
```

Valid only for one of the 40 well-known engine names in `toolchains.md`. Any
other name is rejected when the config loads, with the object form spelled out:

```text
Error: engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
```

The scope in that message is `root` or `workspace '<name>'`.

### Object form

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `version` | `string` | no | none | Constraint, such as `">=13.0.0"`. |
| `versionCmd` | `string` | no | none | Command that prints the tool's version. |
| `installCmd` | `string` | no | none | Command that installs the toolchain. Its presence is what selects provisioning. |
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

A `version` on a name outside the well-known table needs a `versionCmd` beside
it. Config validation does not catch the omission; the first real run does:

```text
Error: engine 'protoc' has a version constraint but no way to check the installed version. 'protoc' is not a well-known engine, so add a `versionCmd` to it
```

A value that is neither a string nor an object is rejected while parsing:

```text
Error: failed to parse lattice.json

Caused by:
    invalid type: integer `20`, expected a version constraint string or an engine object at line 1 column 25
```

A workspace's `engines` map merges with the root's; the workspace wins per key.
See `toolchains.md` for which shape selects which mode and where a provisioned
tool lands.

## The task object

Keys under `tasks` are task names you choose. Declaration order is preserved and
means nothing; the dependency graph decides execution order. An empty task
object `{}` is valid.

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `dependsOn` | array of `string` | no | none | `^task` is that task in each workspace this one depends on. Bare `task` is that task in this same workspace. |
| `inputs` | array of `string` | no | none | Workspace-relative globs whose file contents feed the cache key. |
| `outputs` | array of `string` | no | none | Workspace-relative globs captured as the cached artifact. |
| `ignore` | array of `string` | no | none | Globs subtracted from what `inputs` matched. |
| `env` | array of `string` | no | none | Variable *names* whose resolved values feed the cache key. |
| `persistent` | `boolean` | no | `false` | Never cached, forces raw output for the whole run, must be a graph leaf. Its exit is reported, and a non-zero one fails the run. |
| `cache` | `boolean` | no | `true` | `false` opts a non-persistent task out of caching. |
| `timeout` | `string` or integer | no | none | `"90s"`, `"10m"`, `"1h"`, or a number of seconds. On overrun the task's process group gets `SIGTERM`, five seconds, then `SIGKILL`, and the task counts as failed. Ignored on a `persistent` task. Not part of the cache key. |

`^task` and bare `task` are the whole addressing vocabulary. There is no glob,
no `workspace#task`, and no way to name one specific workspace's task.

`timeout` accepts `ms`, `s`, `sec`, `secs`, `m`, `min`, `mins`, `h`, `hr`, and
`hrs`, case-insensitively, or a bare number read as seconds. A value under one
second rounds up to one second. Zero and negative values are rejected.

Running a task name that is not a key under `tasks`:

```text
Error: task 'build' is not defined in the `tasks` map in lattice.json. Defined tasks: test
```

With an empty `tasks` map the tail reads `lattice.json defines no tasks`.

## Settings

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `maxCacheSize` | `string` | no | none | An integer plus `B`, `KB`, `MB`, `GB`, or `TB`, base 1024, case-insensitive. A bare integer of bytes works as a string. Enforced after every run, and used by `lattice prune` when `--max-size` is absent. Unset, the cache grows without limit. |
| `cacheDir` | `string` | no | `".lattice/cache"` | Local cache directory, relative to the repo root. |
| `loquacious` | `boolean` | no | `false` | Equivalent to always passing `-l`. |
| `versionCheck` | `boolean` | no | `true` | `false` disables the `latticeVersion` handover and the drift nag. |

`maxCacheSize` must be a JSON string. `"10GB"` and `"1048576"` are both valid;
the number `1048576` is not:

```text
Error: failed to parse lattice.json

Caused by:
    invalid type: integer `1048576`, expected a string at line 1 column 65
```

An unrecognized unit is rejected with the unit uppercased in the message, which
also lists the units that are accepted:

```text
Error: failed to parse lattice.json

Caused by:
    unknown cache size unit 'GIGS' in '10 gigs'. Use B, KB, MB, GB, or TB at line 1 column 69
```

## Validation summary

| Problem | When caught | Message |
| --- | --- | --- |
| No `lattice.json` here or in any parent | before parsing | `no lattice.json found in this directory or any parent. Run \`lattice init\` to create one` |
| Malformed JSON | parsing | `failed to parse lattice.json`, with the JSON error and position |
| Workspace missing `name` or `path` | parsing | `missing field \`name\`` or `missing field \`path\``, with line and column |
| A key the config does not define, at any level | parsing | `unknown field \`<key>\` in <container>`, with the position, the nearest valid field, and the fields accepted there |
| Engine value neither a string nor an object | parsing | `invalid type: <what was written>, expected a version constraint string or an engine object` |
| `maxCacheSize` not a string, or a bad unit | parsing | `invalid type: integer \`<n>\`, expected a string`, or `unknown cache size unit '<UNIT>' in '<value>'. Use B, KB, MB, GB, or TB` |
| Workspace `path` empty or whitespace-only | validation | `workspace '<name>' has an empty path` |
| Workspace `path` absolute or drive-prefixed | validation | `workspace '<name>' has a path '<path>' that is not relative to the repo root. Write every workspace path relative to the repo root` |
| Workspace `path` escaping the root with `..` | validation | `workspace '<name>' has a path '<path>' that points outside the repo root. Every workspace path must stay inside the repo` |
| Two workspaces with the same `name` | validation | `duplicate workspace name '<name>'. Every workspace name must be unique` |
| Workspace `dependsOn` naming itself | validation | `workspace '<name>' lists itself in \`dependsOn\`` |
| Workspace `dependsOn` naming an undeclared workspace | validation | `workspace '<name>' depends on '<dep>', which is not a declared workspace`, then `Did you mean \`<near>\`?` and `Declared workspaces: <list>` |
| Task `dependsOn` naming itself | validation | `task '<task>' lists itself in \`dependsOn\`` |
| Task `dependsOn` naming a task not in `tasks` (`^` stripped first) | validation | `task '<task>' depends on '<dep>', but '<dep>' is not defined in \`tasks\``, then `Did you mean \`<near>\`?` and `Defined tasks: <list>` |
| String-form engine that is not well-known | validation | names the engine and its scope, and shows the object form with `versionCmd` |
| Workspace `path` is not an existing directory | discovery | `workspace path '<path>' does not point to a directory. A workspace path is one literal directory, not a glob` |
| Two workspaces resolving to one directory | discovery | `duplicate workspace path '<path>' in lattice.json` |
| Task name not present in `tasks` | after config loads | `task '<name>' is not defined in the \`tasks\` map in lattice.json. Defined tasks: <list>` |
| `auto: false` workspace with no `scripts` entry for a requested task | graph construction | `workspace '<name>' has "auto": false and declares no command for task '<task>'. Add the command under this workspace's "scripts" map in lattice.json` |
| Something depends on a persistent task | graph construction | `task '<task>' in workspace '<ws>' is persistent, so no other task may depend on it` |
| Cycle through `dependsOn` | graph construction | `the task graph has a cycle` |
| An engine that cannot be version-checked | first real run | `engine '<name>' has a version constraint but no way to check the installed version. '<name>' is not a well-known engine, so add a \`versionCmd\` to it` |
| `lattice prune` with no size limit anywhere | after config loads | `no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json` |

Nothing runs when any of these fires. Validation happens before the first task
is spawned.

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
