---
title: CLI reference
description: Every lattice command, every flag, exit codes, and precedence.
group: Reference
order: 2
---

# CLI reference

Every subcommand, argument, flag, and exit code, in the order `lattice --help`
lists them. For the reasoning behind a flag rather than its definition, see
[Selecting what runs](/lattice/docs/filtering), [Output and
logging](/lattice/docs/output-modes), [Caching](/lattice/docs/caching), and
[Upgrading](/lattice/docs/upgrading).

An error prints as one `error:` line, with each cause indented under it, and the
command exits `1`:

```text
error: workspace 'web' cannot run
  engine 'node' is not installed, or not on PATH
    wanted   >=22
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

## Bare `lattice`

`lattice` with no subcommand prints the splash — the ASCII rosette, a version
line, and the tagline — then `Run lattice --help to see available commands.`,
and exits `0`.

## `lattice run`

```text
lattice run [OPTIONS] <TASKS>... [-- <ARGS>...]
```

Runs one or more tasks in every workspace that has them, in dependency order.
A task does not have to be named in `lattice.json`: any script a workspace's
manifest declares, and any verb its tool has, can be run. Naming several tasks
merges them into one graph, so a dependency they share runs once.

**Arguments**

| Argument | Description |
| --- | --- |
| `<TASKS>...` | One or more task names. Required |
| `-- <ARGS>...` | Arguments passed to the tasks you named |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--sequentially` | `-s` | — | off | Run each task's graph to completion in turn, instead of one merged graph |
| `--filter` | `-f` | `<PATTERN>` | none | Run in the workspaces whose name contains this pattern, plus what they depend on |
| `--concurrency` | — | `<N>` | number of CPUs | Cap how many tasks run at once |
| `--continue` | — | — | off | Keep running independent tasks after a failure |
| `--no-cache` | — | — | off | Neither read nor write the cache |
| `--force` | — | — | off | Re-run every task and write fresh cache entries over any stored |
| `--dry-run` | — | — | off | List the tasks that would run, and exit |

Plus the [global flags](#global-flags).

```sh
lattice run build
lattice run lint test build
lattice run test --filter api
lattice run test -- --watch=false
lattice run build --dry-run
lattice run lint --concurrency 4 --continue
```

A name no workspace has is refused before anything runs:

```text
error: no workspace has a 'tset' task. Did you mean 'test'?
Tasks: build, check, clean, dev, format, lint, test
```

**The directory you run from scopes the run.** Inside a workspace, `lattice run
build` covers that workspace and everything it depends on, the same closure
`--filter` produces. The output says so (`in web, the workspace you are in`). Run
from the root, or pass `--filter`, to cover more.

**Arguments after `--`** are appended to the commands of the tasks you named, in
the workspaces the run is for. Prerequisites pulled in do not get them. A script
run through `npm run` gets npm's own `--` first, so `lattice run test --
--watch=false` runs `npm run test -- --watch=false`, and a Taskfile task gets
Task's, as `task test -- --watch=false`. Other commands get the
arguments directly, so a tool's own separator is still yours to write: Cargo's
test harness needs `lattice run test -- -- --nocapture`. An argument is part of
the command, so it is part of the cache key.

`--no-cache` and `--force` both skip the lookup. `--no-cache` writes nothing;
`--force` writes a fresh entry, which is how to replace a bad one.

`--dry-run` prints one line per node with its command, and tags a node that is in
the graph only because something selected depends on it:

```text
❖ lattice  dry run · build
  → core:build (dependency)  cargo build
  → web:build  pnpm run build
```

With `--sequentially` each graph's banner ends in `(phase)`. `--dry-run` checks
no engines.

Before the graph runs, Lattice says what it left out. A workspace that declares
a name one typo from a requested task is a warning:

```text
lattice: warning: web has no 'build' task. Did you mean 'biuld'?
```

The workspaces that simply lack the task are a note, shown on the live display
and in the stream with `-v`. Neither stops the run.

A filter that matches nothing, and a repo with no workspaces, print a line and
exit `0`:

```text
lattice: no workspaces matched filter 'zzz'.
lattice: found no workspaces. The repo root names none in a workspace file and holds no project Lattice recognizes. List workspace directories under `workspaces` in lattice.json, like "apps/*".
```

What a run prints is [Output and logging](/lattice/docs/output-modes).

## `lattice setup`

```text
lattice setup [OPTIONS] [WORKSPACES]...
```

Checks or installs the tool versions under `engines`, then installs each
workspace's dependencies with its tool's installer: `pnpm install`,
`cargo fetch`, `uv sync`, `./gradlew dependencies`, and so on. The full list is
in [Toolchains](/lattice/docs/toolchains#what-lattice-setup-installs).

**Arguments**

| Argument | Description |
| --- | --- |
| `[WORKSPACES]...` | Set up only these workspaces. Omit for all |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--force` | — | — | off | Reinstall even if no lockfile changed |

```sh
lattice setup
lattice setup api web
lattice setup --force
```

A name that is not a workspace is refused before anything runs:

```text
error: there is no workspace named 'wbe'. Workspaces: api, web
```

An installer runs in the nearest directory at or above the workspace that holds
a lockfile, so the members of one pnpm, npm, yarn or Cargo tree share a single
install. It is skipped, with `dependencies up to date`, when no lockfile in the
workspace or above it is newer than that workspace's marker under
`.lattice/setup`. The installer's output streams indented as it runs, and it gets
no stdin, so an installer that asks a question fails with its own message instead
of waiting on a prompt nobody sees.

A workspace with no tool and no engines is skipped. A failed install is a warning
for that workspace, the others still run, and the command then fails.

## `lattice init`

```text
lattice init [OPTIONS]
```

Writes a `lattice.json` holding only what Lattice would not find on its own, a
`.lattice/schema.json` to commit, and these `.gitignore` lines:

```text
.lattice/cache/
.lattice/toolchains/
.lattice/bin/
.lattice/setup/
.lattice-setup-marker
```

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--force` | — | — | off | Overwrite an existing `lattice.json` |
| `--yes` | `-y` | — | off | Write the proposal without prompting |

What `init` proposes:

- **Workspaces no workspace file names.** The outermost directories holding a
  project Lattice recognizes that the root's `package.json` `workspaces`,
  `pnpm-workspace.yaml`, `Cargo.toml`, `go.work`, `pyproject.toml` or `deno.json`
  does not already list. A directory inside another project (a Gradle
  subproject, a fixture package) is left to that project. When two or more
  siblings are every project under their parent, they are written as one glob.
- **Tool versions the root already pins**, as `engines`: `.tool-versions`,
  `.nvmrc` (node), `rust-toolchain.toml` or `rust-toolchain` (rust),
  `.python-version`, `.ruby-version`, `.java-version`, `package.json`
  `packageManager` and `engines`, and `go.mod`'s `toolchain` or `go` line.
  Only [well-known engine](/lattice/docs/toolchains#well-known-engines) names
  are proposed.
- **Task settings a `turbo.json` spells out** — each task's `dependsOn`,
  `inputs`, `outputs`, `env`, `persistent` and `cache`, and the file's
  `globalDependencies` and `globalEnv` — when the directory holding it is a
  proposed workspace or the repo root, including a monorepo root whose
  `package.json` names the workspaces.

A task a file only names, as a `package.json` script does, is not written: it
runs by name as it is.

On a terminal, `init` reports how many workspaces the repo's workspace files
already name, then shows the extra workspaces and the pins as checklists. With
`--yes`, or with no terminal, it writes the proposal. Either way it then prints
what Lattice now sees:

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

3 workspaces:
  @acme/ui  npm      build lint
  web       npm      build dev test
  api       go       build test check lint format clean

next: lattice run build
```

`updated .gitignore` appears only when a line was missing. A proposed directory
Lattice cannot pick a tool for is named at the end, with the `driver` override
that would bring it in. `init` refuses an existing `lattice.json` without
`--force`, and points a 1.x one at `lattice migrate`.

## `lattice migrate`

```text
lattice migrate [OPTIONS]
```

Rewrites a Lattice 1.x `lattice.json` for this version. It runs discovery against
the repo and writes down only where the result would differ from what the 1.x
config said, so every workspace keeps its name, every ordering holds, and every
command still runs. The mapping is in [Upgrading to
2.0](/lattice/docs/upgrading#upgrading-to-20).

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--dry-run` | — | — | off | Print the new `lattice.json` instead of writing it |

```text
✓ rewrote lattice.json 64 lines → 21
· dropped the `pnpm` engine: its constraint matched any version, so it only chose a driver, and drivers are chosen with `driver` now
· web: dropped dependsOn ui — its manifest already says so
```

With `--dry-run` the file goes to stdout and the notes to stderr. On a current
config it prints `lattice.json is already written for this version. Nothing to
do.` `migrate` runs as the binary you invoked, never handed to a pinned
`latticeVersion`, and sets `latticeVersion` to its own version.

## `lattice env`

```text
lattice env [OPTIONS]
```

Prints the `PATH` a task would see, as a command your shell can evaluate: the
`bin` of every tool Lattice installed for the repo, then (for a workspace) the
directories its package manager installed tools into, such as
`node_modules/.bin`. Engines are checked and installed first, the way a run
does.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--workspace` | `-w` | `<NAME>` | the workspace you are in | Whose `PATH` to print. Outside any workspace, the root's engines |
| `--shell` | — | `posix` \| `fish` \| `powershell` \| `cmd` | from `$SHELL` | The syntax to print |

```sh
eval "$(lattice env)"
lattice env --shell fish | source
lattice env --workspace api
```

```text
export PATH="/repo/.lattice/toolchains/node/22.11.0-1a2b3c4d/bin:/repo/node_modules/.bin:$PATH"
```

With nothing to add it prints nothing, and says why on stderr. An engine checked
against the host tool adds nothing (the tool is already on `PATH`), but still has
to pass its check.

## `lattice prune`

```text
lattice prune [OPTIONS]
```

Evicts cache entries, least recently used first, until the cache is under a size
limit: `--max-size`, else `settings.maxCacheSize`.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--max-size` | — | `<SIZE>` | `settings.maxCacheSize` | Upper bound, such as `10GB` |

```text
❖ removed 12 artifacts, freed 1.2GB
```

A size is a number and `B`, `KB`, `MB`, `GB` or `TB` (base 1024), or bare bytes.
With no limit anywhere, `prune` fails rather than guess. It also reclaims what
nothing can read — an artifact left without metadata by an interrupted run,
metadata that does not parse, staging files — once it has sat untouched for an
hour, so two `lattice` processes can share one cache without deleting each
other's writes. It deletes only `*.tar.gz`, `*.meta.json` and `*.tmp` files in
the cache directory. See [Cache internals](/lattice/docs/cache-internals).

## `lattice stats`

```text
lattice stats
```

Reports what the cache has done for this repo. No flags of its own.

```text
❖ lattice  stats  since 2026-05-02

  saved      3h 41m of task time
  runs       412 · 2,904 of 3,390 tasks cached (86%)
  cache      1.4 GB · 892 entries
  last 7d    27m 04s saved across 38 runs
```

`saved` is task time, not wall clock. The record is `stats.jsonl` in the cache
directory, one line appended per run that could store and scheduled a task, so a
`--no-cache` run records nothing. It is per-machine, `prune` leaves it alone, and
clearing the cache clears it. With nothing recorded: `No runs recorded yet.`

## `lattice upgrade`

```text
lattice upgrade [OPTIONS] <VERSION>
```

Installs another version into `.lattice/bin`, points `.lattice/bin/lattice` at
it, and writes it to `latticeVersion`. `<VERSION>` is a version such as `0.2.0`,
or `latest`.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--release-latest-url` | — | `<URL>` | GitHub API | Names the newest stable release, for `upgrade latest` |
| `--release-list-url` | — | `<URL>` | GitHub API | Lists every release, used when none is stable yet |

See [Upgrading](/lattice/docs/upgrading).

## `lattice completions`

```text
lattice completions <SHELL>
```

Prints a completion script for `bash`, `elvish`, `fish`, `powershell` or `zsh`.

```sh
lattice completions zsh > ~/.zsh/completions/_lattice
```

## `lattice version`

```text
lattice version [--json]
```

Prints the splash, or one line of JSON:

```json
{"version":"2.0.0","target":"aarch64-apple-darwin","arch":"aarch64"}
```

## Global flags

These parse on `lattice` and on every subcommand, before or after its name.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--verbose` | `-v` | — | off | Show more: why each task missed the cache, its key, and per-task notes |
| `--stream` | — | — | off | Print a plain `workspace:task` line per event instead of the live display |
| `--no-version-check` | — | — | off | Run this binary even when the repo pins another version |
| `--theme` | — | `light` \| `dark` | detected | Shade the logo for a light or dark terminal |
| `--release-base-url` | — | `<URL>` | GitHub releases | Where release archives download from. A `file://` base works offline |

`-v` adds detail to whichever display the run uses; it no longer switches
displays. `--stream` does that, and a run already gets the stream when stdout is
not a terminal, when `CI` is set, or when it includes a persistent task.
`--loquacious` (`-l`) is a hidden alias for `--verbose`.

`-V` and `--version` print the binary's version and exist only on `lattice`
itself.

## Option precedence

Highest first:

| Source | Examples |
| --- | --- |
| CLI flag | `-v`, `--stream`, `--no-version-check`, `--theme`, `--release-base-url`, `--max-size` |
| Environment variable | `CI`, `LATTICE_NO_VERSION_CHECK`, `LATTICE_THEME`, `LATTICE_RELEASE_BASE_URL` |
| `settings` in `lattice.json` | `settings.loquacious`, `settings.versionCheck`, `settings.maxCacheSize` |
| Built-in default | — |

See [Environment variables](/lattice/docs/environment-variables).

## Exit codes

| Exit code | Meaning |
| --- | --- |
| `0` | Success, including a filter that matched nothing and a repo with no workspaces |
| `1` | Any error Lattice raises, including a failed task |
| `2` | The command line was rejected: an unknown subcommand or flag, a bad value, a missing argument |
| `130` | A run was interrupted by Ctrl-C or `SIGTERM` |

Without `--continue`, the first failing task ends the run, and tasks already
running finish:

```text
error: task 'app:build' failed, stopping the run
```

With `--continue`, independent tasks keep going, anything waiting on a failure is
skipped, and the run exits `1` after its summary. `--sequentially` applies the
rule per phase.

On an interrupt, every running task's process group gets `SIGTERM`, five
seconds, then `SIGKILL`, and the process exits `130` with `error: interrupted.
Lattice stopped the tasks that were still running`. A task stopped that way is
not counted as failed. `130` is distinct from `1` so a cancelled CI job does not
read as a broken build.

Every message is in [Errors](/lattice/docs/errors).
