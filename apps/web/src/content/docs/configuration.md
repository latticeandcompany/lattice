---
title: Configuration
description: Field reference for lattice.json, with types, defaults, and what an invalid value does.
group: Reference
order: 1
---

# Configuration

`lattice.json` at the repo root is the only file Lattice reads for
configuration. This page lists every field it accepts, with its type, whether it
is required, its default, and what an invalid value does. For what the fields
mean in practice, see [Workspaces](/lattice/docs/workspaces), [Task
graph](/lattice/docs/task-graph), [Caching](/lattice/docs/caching), and [Engines
and provisioning](/lattice/docs/engines).

## Top-level fields

Every top-level key is optional, so `{}` is a valid `lattice.json`.

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| [`$schema`](#schema) | `string` | no | none |
| [`latticeVersion`](#latticeversion) | `string` | no | none |
| [`workspaces`](#workspaces) | array of workspace objects | no | `[]` |
| [`engines`](#engines) | engine map | no | `{}` |
| [`globalDependencies`](#globaldependencies) | array of `string` | no | `[]` |
| [`globalEnv`](#globalenv) | array of `string` | no | `[]` |
| [`tasks`](#tasks) | object of `string` to task object | no | `{}` |
| [`settings`](#settings) | settings object | no | all defaults |

## Where the file lives

Every command that reads config walks up from the current directory to the
nearest `lattice.json` and treats that directory as the repo root. Commands
therefore work from any subdirectory. With no `lattice.json` in the current
directory or any parent, the command fails before reading anything else:

```text
Error: no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

## `$schema`

```json
{ "$schema": ".lattice/schema.json" }
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `$schema` | `string` | no | none |

A plain string reference, conventionally `.lattice/schema.json`. That path holds
a copy of Lattice's bundled JSON Schema, written next to your config so that
editors with JSON Schema support validate and autocomplete `lattice.json` as you
type. Lattice never reads the value.

`lattice run`, `lattice setup`, `lattice prune`, and `lattice stats` write
`.lattice/schema.json` only when the file is absent, so a copy you have pinned
or hand-edited stays as it is. `lattice init` writes it unconditionally as part
of scaffolding, along with five `.gitignore` lines: `.lattice/cache/`,
`.lattice/toolchains/`, `.lattice/bin/`, `.lattice/setup/`, and
`.lattice-setup-marker`.
`.lattice/schema.json` is not among them.

## Unknown keys

The bundled schema sets `additionalProperties: false` at every level, and the
parser agrees: a key Lattice does not recognize fails the load before any task
is scheduled.

```text
Error: unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

The message names the key, the object it sits in, its position in the file, the
closest accepted field, and every field accepted there. A workspace entry is
indexed, as in `workspaces[1]`, so the message points at one entry out of
several.

There is no way to keep an extra key in the file. A `lattice.json` carrying a
key from an earlier release, or a note under a key of your own, has to drop it.

## `latticeVersion`

```json
{ "latticeVersion": "1.0.0" }
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `latticeVersion` | `string` | no | none |

The version of Lattice this repo runs. A non-string value fails to parse. The
string itself is not validated at load time. See
[Upgrading](/lattice/docs/upgrading) for how the version check uses this field
together with `settings.versionCheck`.

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

Each entry is one directory with its own manifest. There is no glob form: a bare
string array such as `"workspaces": ["apps/*"]` fails to parse with
`invalid type: string "apps/*", expected struct WorkspaceConfig`. A `glob` key
on an entry fails as an unknown field.

### The workspace object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | `string` | yes | none | Workspace name. Unique across the file. |
| `path` | `string` | yes | none | Literal directory path, relative to the repo root. Non-empty. |
| `auto` | `boolean` | no | `true` | With `true`, infer the driver and task commands from the workspace's native manifest. With `false`, infer nothing and take commands from `scripts` alone. |
| `engines` | engine map | no | `{}` | Per-workspace engine constraints. A key here replaces the same key at the root. |
| `dependsOn` | array of `string` | no | none | Other workspaces, by name, this one depends on. |
| `scripts` | object of `string` to `string` | no | `{}` | Task name to shell command. Overrides anything inferred. Every key must name a task declared under the root `tasks`. |

Rejected values:

| Value | Result |
| --- | --- |
| No `name`, or no `path` | ``missing field `name` `` at parse time, with line and column |
| `path` empty or whitespace-only | `workspace '<name>' has an empty path` |
| `path` absolute, or starting with a drive letter | `workspace '<name>' has a path '<path>' that is not relative to the repo root. Write every workspace path relative to the repo root` |
| `path` climbing above the repo root with `..` | `workspace '<name>' has a path '<path>' that points outside the repo root. Every workspace path must stay inside the repo` |
| Two entries with the same `name` | `duplicate workspace name '<name>'. Every workspace name must be unique` |
| Two entries resolving to the same directory | `duplicate workspace path '<path>' in lattice.json` |
| `path` naming something that is not a directory | `workspace path '<path>' does not point to a directory. A workspace path is one literal directory, not a glob` |
| `dependsOn` naming the workspace itself | ``workspace '<name>' lists itself in `dependsOn` `` |
| `dependsOn` naming an undeclared workspace | `workspace '<name>' depends on '<dep>', which is not a declared workspace`, plus the nearest name and the full list |
| `path` with leading or trailing whitespace around a component | ``workspace '<name>' has a path '<path>' with leading or trailing whitespace around a directory name. Windows drops that whitespace and unix keeps it, so the path would name a different directory on each. Remove it`` |
| A `scripts` key that is not a declared task | ``workspace '<name>' declares a script '<key>', but '<key>' is not defined in `tasks`, so nothing would ever run it``, plus the nearest task name and the full list |
| The same key twice in `engines` or `scripts` | ``duplicate key `<key>` in workspaces[<n>].scripts``, with the position |

Path checks read the string as text rather than through the host's path rules,
so a `lattice.json` that is rejected on one platform is rejected on all of them.
Lattice rejects whitespace around a path component for the same reason. Windows
strips that whitespace and unix keeps it, so one string would name two
directories.

A `scripts` key supplies the command for the root task of the same name and
nothing else, so a key outside `tasks` never runs. Lattice raises an error rather
than accepting such a key, because the workspace would otherwise run its detected
command with nothing said about the override. Every key under `scripts` pairs
with a task of the same name:

```json
{
  "workspaces": [
    { "name": "core", "path": "libs/core", "scripts": { "build": "make all" } }
  ],
  "tasks": { "build": {} }
}
```

A minimal workspace that declares its own commands:

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

An engine map is a name-keyed object whose values are either a bare version
constraint string or an engine object. Root `engines` apply to every workspace,
and a workspace's own entry replaces the root entry of the same name. Which
fields a value carries selects host mode, validate-only, or provisioning. See
[Engines and provisioning](/lattice/docs/engines) for the three modes.

A value that is neither a string nor an object fails to parse:

```text
Error: failed to parse lattice.json

Caused by:
    invalid type: integer `123`, expected a version constraint string or an engine object at line 1 column 22
```

### String form

```json
{ "engines": { "node": ">=20.0.0" } }
```

The bare string form works only for the 40 well-known engine names, which are
the names Lattice has a built-in version command for. Every built-in driver is
here, plus the language toolchains those drivers sit on top of:

| Ecosystem | Names |
| --- | --- |
| JavaScript and TypeScript | `node`, `deno`, `bun`, `pnpm`, `yarn`, `npm` |
| Rust | `rust`, `cargo` |
| Go | `go` |
| Python | `python`, `python3`, `pip`, `uv`, `poetry`, `pdm`, `pipenv` |
| Ruby | `ruby`, `bundler`, `rake` |
| The JVM | `java`, `kotlin`, `gradle`, `maven` |
| .NET | `dotnet`, `nuget` |
| Swift and Objective-C | `swift`, `pod` |
| PHP | `php`, `composer` |
| Elixir | `elixir`, `mix` |
| Dart | `dart` |
| Haskell | `haskell`, `ghc`, `stack`, `cabal` |
| Task runners | `just`, `task`, `turbo`, `nx` |

Each name's version command is on
[Toolchains](/lattice/docs/toolchains#well-known-engines).

A string-form engine outside that list fails validation, before any task runs:

```text
Error: engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not a well-known engine Lattice can version-check on its own. Use the object form with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
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
| `version` | `string` | no | none | Version constraint, such as `">=13.0.0"`. A loose value such as `"1.22"` is read as a lower bound. |
| `versionCmd` | `string` | no | none | Command that prints the tool's version. Required for any name outside the well-known list. |
| `installCmd` | `string` | no | none | Command that installs the toolchain. Its presence selects provisioning mode. It receives `$LATTICE_TOOLCHAIN_DIR` both as an environment variable and by literal substitution into the command string. |
| `bin` | `string` | no | `"bin"` | Bin directory inside the toolchain install, prepended to the task's `PATH`. Read in provisioning mode only. Must be relative, non-empty, and stay inside the install. |

The object form accepts any engine name. A name outside the well-known list that
carries a `version` but no `versionCmd` parses, then fails when Lattice needs
the version:

```text
engine 'alpes' has a version constraint but no way to check it (not a well-known engine and no `versionCmd`)
```

Lattice joins `bin` to the toolchain's install directory and puts the result at
the front of the `PATH` of every task that resolves the engine. The check
therefore runs when the config loads: `bin` has to be relative, non-empty, free
of whitespace around any component, and inside the install directory. `"bin"`,
`"."`, `"usr/local/bin"`, and `"bin/../libexec"` all pass. `"/usr/bin"` and
`"../../.."` are rejected. Either one would put a directory Lattice never
provisioned in front of every command, while the run still reported a provisioned
toolchain:

```text
engine 'node' in root has a `bin` of '/usr/bin', which is not relative to the toolchain install. Write `bin` as a path inside the toolchain directory, like "bin"
```

## `globalDependencies`

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**", ".env"]
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `globalDependencies` | array of `string` | no | `[]` |

Globs relative to the repo root, hashed into the cache key of every task. A
task's `inputs` are relative to its own workspace, so a file above the workspace
cannot be named there in a way that means the same thing everywhere. Editing
anything matched here makes every task in the repo miss. Lattice hashes these
patterns once at the start of a run, so a malformed pattern fails the whole run
rather than one task. See [Cache internals](/lattice/docs/cache-internals).

`lattice init` writes this key when a workspace's `turbo.json` declares
`globalDependencies`. Prune what it brought over on the rule above: a pattern
that does not really cross workspaces costs every task a miss.

## `globalEnv`

```json
{
  "globalEnv": ["NODE_ENV", "CI"]
}
```

| Field | Type | Required | Default |
| --- | --- | --- | --- |
| `globalEnv` | array of `string` | no | `[]` |

Environment variable names whose resolved values feed the cache key of every
task. A name with no value is hashed as declared-and-unset, which is a different
key from not listing the name at all. Unlike a task's `env`, these names are not
set on task processes: they are already in the environment Lattice inherited. A
task's own `env` list applies on top of this one. See [Environment
variables](/lattice/docs/environment-variables).

`lattice init` writes this key too, when a workspace's `turbo.json` declares
`globalEnv`.

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
| `tasks` | object of `string` to task object | no | `{}` |

Keys are task names you choose. Declaration order is preserved and does not
affect execution order, which comes from the dependency graph. Passing a name
that is not a key here fails immediately:

```text
Error: task 'build' is not defined in the `tasks` map in lattice.json. Defined tasks: test
```

With no tasks declared at all, the same error ends with
`lattice.json defines no tasks`.

### The task object

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `dependsOn` | array of `string` | no | none | Task dependencies. `^task` means that task in this task's dependency workspaces. A bare `task` means that task in the same workspace. |
| `inputs` | array of `string` | no | the whole workspace | Globs for the files whose contents feed the cache key. Omitted, every file in the workspace is hashed except what `.gitignore` excludes and what this task's own `outputs` match. |
| `outputs` | array of `string` | no | none | Globs for the files captured as the cached artifact. The files these match are excluded from input hashing. The pattern strings themselves are part of the cache key. |
| `ignore` | array of `string` | no | none | Globs excluded from input hashing. |
| `env` | array of `string` | no | none | Environment variable names whose resolved values feed the cache key. Lattice also sets these on the task's process. |
| `persistent` | `boolean` | no | `false` | A task not expected to exit, such as a dev server. Never cached, whatever `cache` says. See [Persistent tasks](/lattice/docs/persistent-tasks). |
| `cache` | `boolean` | no | `true` | Set `false` to opt a non-persistent task out of caching. |
| `timeout` | `string` or `integer` | no | none | How long the task may run before Lattice stops it and counts it as failed. Maximum 365 days. Ignored on a `persistent` task. |

`timeout` accepts `ms`, `s`, `sec`, `secs`, `m`, `min`, `mins`, `h`, `hr`, and
`hrs`, case-insensitive, or a bare whole number of seconds. A string value below
one second rounds up to one second. An unrecognized unit fails to parse:

```text
Error: failed to parse lattice.json

Caused by:
    unknown duration unit 'fortnights' in '5 fortnights' at line 1 column 43
```

The maximum is 365 days, `31536000` seconds, in both forms. Lattice rejects an
oversized value rather than clamping it. Past 365 days the deadline overflows
instead of arriving, so an oversized `timeout` would mean no timeout at all:

```text
duration '99999h' is longer than the maximum of 365 days. Use a shorter duration, or leave `timeout` out to let the task run without a limit
```

The number form is an integer, the way the bundled JSON schema has always
declared it. Lattice rejects `1.5` rather than rounding it up to `2`. The string
form takes decimals, so `"1.5m"` and `"90s"` both parse. `90.0` counts as a whole
number of seconds:

```text
duration 1.5 is not a whole number of seconds. Write a whole number of seconds, or a duration string such as "90s", "1500ms", or "10m"
```

A glob in `inputs`, `outputs`, `ignore`, or `globalDependencies` is compiled
when the task's cache key is computed rather than when the config loads, so a
malformed pattern fails that one task:

```text
a:build: failed to compute cache key: error parsing glob 'src/[[': unclosed character class; missing ']'
```

A `dependsOn` entry naming a task the map does not define fails validation, and
so does a task that names itself:

```text
Error: task 'build' depends on 'tset', but 'tset' is not defined in `tasks`
Defined tasks: build, test
```

```text
Error: task 'build' lists itself in `dependsOn`
```

An empty task object is valid. It declares no dependencies, no inputs, and no
outputs, and it still caches: its key covers the whole workspace, so it re-runs
whenever anything in that directory changes.

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
| `maxCacheSize` | `string` | no | none | Upper bound on the local cache size. Enforced after every run, and used by `lattice prune` when `--max-size` is absent. Unset, the cache grows without limit. |
| `cacheDir` | `string` | no | `".lattice/cache"` | Directory for the local cache, relative to the repo root. Must be non-empty, stay inside the repo, and not be the repo root itself. |
| `loquacious` | `boolean` | no | `false` | With `true`, always use raw output, as `-v`/`--verbose` does. |
| `versionCheck` | `boolean` | no | `true` | With `true`, compare the running binary against `latticeVersion`. With `false`, skip the check. See [Upgrading](/lattice/docs/upgrading). |

`maxCacheSize` is a string: an integer or decimal followed by `B`, `KB`, `MB`,
`GB`, or `TB`, base 1024 and case-insensitive. `K`, `M`, `G`, and `T` are
accepted as short forms, and a string of digits with no unit is read as bytes.

```json
{ "settings": { "maxCacheSize": "512MB" } }
```

A JSON number fails to parse, even though the same digits work as a string:

```text
Error: failed to parse lattice.json

Caused by:
    invalid type: integer `536870912`, expected a string at line 1 column 41
```

An unrecognized unit fails the same way:

```text
Error: failed to parse lattice.json

Caused by:
    unknown cache size unit 'QB' in '512QB' at line 1 column 36
```

With `settings.maxCacheSize` unset, `lattice prune` needs `--max-size`:

```text
Error: no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json
```

Lattice validates `cacheDir` when the config loads, and owns the directory
`cacheDir` names: `lattice prune` deletes the `*.tar.gz`, `*.meta.json`, and
`*.tmp` files it finds there. The value therefore has to be relative, has to stay
inside the repo, and cannot resolve to the repo root. `".lattice/cache"` and
`".cache/lattice"` both pass. `"."` does not, because it would point `prune` at
the top of the repo:

```text
Error: `settings.cacheDir` is '.', which is the repo root itself. Point it at a directory of its own, like ".lattice/cache" — `lattice prune` deletes cache archives and partial writes in whatever directory it names
```

`"/tmp/lattice"` and `"../cache"` are rejected as well, each with its own
message. See [Errors](/lattice/docs/errors) for all five.

## When each check runs

| Problem | Stage | Error |
| --- | --- | --- |
| No `lattice.json` in this or any parent directory | before parsing | ``no lattice.json found in this directory or any parent. Run `lattice init` to create one`` |
| Malformed JSON | parsing | `failed to parse lattice.json`, with the underlying JSON error and its position |
| A workspace object missing `name` or `path` | parsing | ``missing field `name` ``, with line and column |
| A key Lattice does not recognize, at any level | parsing | ``unknown field `<key>` in <path>``, with position, the nearest valid field, and the fields accepted there |
| A value of the wrong JSON type | parsing | `invalid type: <what was written>, expected <what the field takes>` |
| An engine value that is neither a string nor an object | parsing | `invalid type: <what was written>, expected a version constraint string or an engine object` |
| An unrecognized `timeout` or `maxCacheSize` unit | parsing | `unknown duration unit '<unit>' in '<value>'`, `unknown cache size unit '<unit>' in '<value>'` |
| A `timeout` over 365 days, or a fractional number of seconds | parsing | ``duration '<value>' is longer than the maximum of 365 days``, `duration of <n> seconds is longer than the maximum of 365 days`, `duration <n> is not a whole number of seconds` |
| The same key twice in `tasks`, `engines`, or a workspace's `engines` or `scripts` | parsing | ``duplicate key `<key>` in <container>``, with the position |
| A string-form engine whose name is not well-known | validation | names the engine and suggests the object form with `versionCmd` |
| A workspace `path` that is empty or whitespace-only | validation | `workspace '<name>' has an empty path` |
| A workspace `path` that is absolute or climbs above the repo root | validation | `... is not relative to the repo root`, `... points outside the repo root` |
| A workspace `path` with whitespace around a component | validation | `... with leading or trailing whitespace around a directory name` |
| A `settings.cacheDir` that is empty, absolute, outside the repo, or the repo root | validation | names the field, quotes the value, and says which rule it broke |
| An engine `bin` that is empty, absolute, or outside the toolchain install | validation | ``engine '<name>' in <scope> has a `bin` of '<value>', which ...`` |
| A workspace `scripts` key that names no declared task | validation | ``workspace '<name>' declares a script '<key>', but '<key>' is not defined in `tasks` ``, with the nearest name and the full list |
| Two workspaces with the same `name` | validation | `duplicate workspace name '<name>'. Every workspace name must be unique` |
| A workspace or task that lists itself in `dependsOn` | validation | ``workspace '<name>' lists itself in `dependsOn` ``, ``task '<name>' lists itself in `dependsOn` `` |
| A workspace `dependsOn` naming an undeclared workspace | validation | `workspace '<name>' depends on '<dep>', which is not a declared workspace`, with the nearest name and the full list |
| A task `dependsOn` naming a task not in `tasks` | validation | ``task '<name>' depends on '<dep>', but '<dep>' is not defined in `tasks` ``, with the nearest name and the full list |
| A workspace `path` that is not a directory | workspace discovery | `workspace path '<path>' does not point to a directory. A workspace path is one literal directory, not a glob` |
| Two workspaces resolving to one directory | workspace discovery | `duplicate workspace path '<path>' in lattice.json` |
| An `auto` workspace with no unambiguous driver | workspace discovery | `workspace '<name>' has an ambiguous or undeclared driver.`, with candidates and a fix |
| A task name passed to `lattice run` that is not in `tasks` | after config loads | ``task '<name>' is not defined in the `tasks` map in lattice.json``, with the defined task names |
| `lattice prune` with no size limit anywhere | after config loads | `no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json` |
| An engine that fails its version constraint | before any task runs | `engine '<name>' on PATH is <version>, which does not satisfy the constraint '<constraint>'` |
| A malformed glob in `inputs`, `outputs`, `ignore`, or `globalDependencies` | while running | `failed to compute cache key: error parsing glob ...` |

Everything at the parsing and validation stages is raised before Lattice looks
at a single workspace directory, so nothing has run when you see it.

## Complete example

A repo with a Node app, a Rust service, and a Python library:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0",
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
