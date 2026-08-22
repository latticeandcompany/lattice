---
title: CLI reference
description: Every lattice command, every flag, exit codes, and precedence.
group: Reference
order: 2
---

# CLI reference

Every subcommand, every argument, every flag, and every exit code. The
descriptions in the tables below are the text `lattice <command> --help` prints.
Where this page and `--help` disagree, `--help` is right and this page is a docs
bug.

Sections appear in the order the commands appear in `lattice --help`. For the
reasoning behind a flag rather than its definition, see [Selecting what
runs](/lattice/docs/filtering), [Caching](/lattice/docs/caching), [Engines and
provisioning](/lattice/docs/engines), and [Upgrading](/lattice/docs/upgrading).

## Bare `lattice`

`lattice` with no subcommand prints the same splash as `lattice version`, then a
line pointing at `--help`, then exits `0`. There is no "missing subcommand"
error.

```sh
lattice
```

The splash is the ASCII rosette, then a version line and the tagline. Without
the rosette art, those two lines are:

```text
❖ lattice  1.0.0-beta-2  (aarch64)
A high-performance, local toolchain for managing monorepos.
```

Then a blank line and:

```text
Run lattice --help to see available commands.
```

## `lattice run`

```text
lattice run [OPTIONS] <TASKS>...
```

Runs one or more tasks across your workspaces, in dependency order. Lattice
builds each task's dependency graph from the `tasks` map in `lattice.json`, then
runs that graph in dependency order. Naming several tasks at once merges them
into one graph, so a dependency they share runs once.

Every name in `<TASKS>` must be a key of the `tasks` map. Lattice checks all of
them before it builds a graph, provisions a toolchain, or spawns a process.

**Arguments**

| Argument | Description |
| --- | --- |
| `<TASKS>...` | One or more task names, separated by spaces. Required; at least one. |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--sequentially` | `-s` | — | off | Run each task's graph to completion in turn, instead of merging them into one combined graph |
| `--filter` | `-f` | `<PATTERN>` | none | Run in the workspaces whose name contains this pattern, plus what they depend on |
| `--concurrency` | — | `<N>` | number of CPUs | Cap how many tasks run at once. The default is the number of CPUs |
| `--continue` | — | — | off | Keep running independent tasks after a failure instead of stopping |
| `--no-cache` | — | — | off | Neither read nor write the cache. Lattice re-runs every task and stores nothing |
| `--force` | — | — | off | Re-run every task and write fresh cache entries, replacing any already stored |
| `--dry-run` | — | — | off | List the tasks that would run, then exit without running them |

`--concurrency` has no short alias.

Plus the [global flags](#global-flags) below.

```sh
lattice run build
lattice run lint test build
lattice run lint test build --sequentially
lattice run test --filter api
lattice run lint --concurrency 4 --continue
```

`--no-cache` and `--force` both skip the lookup. They differ in what they leave
behind: `--no-cache` writes nothing, and `--force` writes a fresh entry over
whatever was stored at that key.

`--dry-run` prints one line per task in the graph, each with the command that
task would run:

```text
❖ lattice  dry run · build
  → core:build  cargo build
  → api:build  go build
  → web:build  pnpm run build
```

A task that is in the graph only because a selected workspace depends on it
carries a `(dependency)` tag:

```text
❖ lattice  dry run · build
  → core:build (dependency)  cargo build
  → web:build  pnpm run build
```

With `--sequentially`, the banner names the task whose graph follows and ends in
`(phase)`:

```text
❖ lattice  dry run · build (phase)
```

A `--filter` that matches no workspace prints a message and exits `0`:

```text
lattice: no workspaces matched filter 'zzz'.
```

So does a repo whose `workspaces` array is empty:

```text
lattice: no workspaces declared. Add one to the `workspaces` array in lattice.json, then run `build`.
```

In that message, the backticks at the end hold the task names you passed, joined
by spaces. Neither message is a failure. See [Selecting what
runs](/lattice/docs/filtering) for how `--filter` and `--dry-run` shape the
graph, and [Persistent tasks](/lattice/docs/persistent-tasks) for why a
persistent task in the graph forces raw output whatever `-l` says.

## `lattice setup`

```text
lattice setup [OPTIONS] [WORKSPACES]...
```

Provisions pinned toolchains, then installs each workspace's dependencies.
Lattice provisions the toolchains declared under `engines` first, into
`.lattice/toolchains`, so every dependency installer runs with the pinned
`PATH`. Each workspace's package manager then installs that workspace's
dependencies. A repo that declares no workspaces still gets its `engines`
provisioned.

Each workspace's install command comes from its detected driver: `pnpm install`,
`cargo fetch`, `poetry install`, and so on.

**Arguments**

| Argument | Description |
| --- | --- |
| `[WORKSPACES]...` | Set up only the workspaces named here. Omit to set up all of them. |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--force` | — | — | off | Reinstall dependencies even if the lockfile has not changed |

Plus the [global flags](#global-flags) below.

```sh
lattice setup
lattice setup api web
lattice setup --force
```

Setup skips a workspace's install step when no lockfile in it is newer than
`.lattice-setup-marker`, the empty file Lattice writes there after a successful
install. `--force` reinstalls regardless. A workspace with no detected driver
and no `engines` is skipped without a message. One with `engines` but no package
manager reports that under `-l`:

```text
lattice: web: toolchains ready. This workspace has no package manager to install
```

A failed install is a per-workspace warning, and the remaining workspaces still
run. The command then fails as a whole. See [`lattice setup`
failures](/lattice/docs/errors#lattice-setup-failures).

## `lattice init`

```text
lattice init [OPTIONS]
```

Creates a `lattice.json` and a `.lattice/schema.json` in the current directory.
Commit the schema file. `init` also adds three lines to `.gitignore`, which keep
Lattice's per-machine artifacts out of version control:

```text
.lattice/cache/
.lattice/toolchains/
.lattice/bin/
```

`init` reads the repo before it writes anything. Every directory holding one of
these manifests becomes a proposed workspace:

```text
package.json      pyproject.toml     pom.xml            pubspec.yaml
Cargo.toml        requirements.txt   build.gradle       Package.swift
go.mod            setup.py           build.gradle.kts   stack.yaml
Gemfile           composer.json      mix.exs            cabal.project
```

A directory holding a `.sln`, `.csproj`, `.fsproj`, or `.vbproj` file counts
too. Those are matched by extension, because their filenames vary. Every tool
version the repo already pins becomes a proposed
[engine](/lattice/docs/engines):

| File | Engine |
| --- | --- |
| `.tool-versions` | every well-known tool named in it |
| `.nvmrc` | `node` |
| `rust-toolchain.toml`, `rust-toolchain` | `rust` |
| `.python-version` | `python` |
| `.ruby-version` | `ruby` |
| `.java-version` | `java` |
| `package.json` `packageManager` | the tool named there |
| `package.json` `engines` | each tool named there |
| `go.mod`, its `toolchain` line or its `go` directive | `go` |

The walk skips hidden directories, anything `.gitignore` covers, and these
directory names: `node_modules`, `target`, `dist`, `build`, `out`, `vendor`,
`venv`, `__pycache__`, `coverage`, `testdata`, `fixtures`.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--force` | — | — | off | Overwrite an existing `lattice.json` |
| `--yes` | `-y` | — | off | Write what the scan finds without prompting |

Plus the [global flags](#global-flags) below.

```sh
lattice init
lattice init --yes
lattice init --force
```

On a terminal, `init` shows the two lists with everything pre-checked and lets
you uncheck what is wrong. A repo root that holds only a workspace declaration,
such as a `Cargo.toml` with `[workspace]` or a `package.json` with
`workspaces`, is offered alongside its members but starts unchecked. So is a
directory whose driver stays ambiguous, because declaring it would halt the next
run on the [ambiguity](/lattice/docs/drivers#when-lattice-halts). `init` names
those directories on the way out:

```text
· left out apps/legacy. No driver resolved there. Declare one in engines to add it.
```

With `--yes`, or with no terminal attached, `init` writes what the scan found and
prompts for nothing. A scan that finds nothing then writes the bare skeleton. On
a terminal, unchecking everything makes `init` ask for at least one workspace or
one engine rather than writing a config that does nothing.

Running `init` where a `lattice.json` already exists is an error unless you pass
`--force`.

## `lattice prune`

```text
lattice prune [OPTIONS]
```

Evicts cache artifacts, oldest first, until the local cache is under a size
limit. The limit comes from `--max-size`. Without that flag, Lattice uses
`settings.maxCacheSize` in `lattice.json`.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--max-size` | — | `<SIZE>` | `settings.maxCacheSize` | Upper bound on the cache size, such as 10GB. Defaults to settings.maxCacheSize |

Plus the [global flags](#global-flags) below.

```sh
lattice prune
lattice prune --max-size 5GB
```

A size is a number and one of `B`, `KB`, `MB`, `GB`, or `TB`, case-insensitive
and base 1024, or a bare integer of bytes. With neither `--max-size` nor
`settings.maxCacheSize`, `prune` fails rather than guessing a limit.

```text
❖ removed 0 artifacts, freed 0B
```

`prune` also reclaims what nothing can read: artifacts left without metadata by
an interrupted run, metadata that no longer parses, and the staging files beside
them. With `settings.maxCacheSize` set, every run already holds the cache to it,
so `prune` covers sweeping by hand and enforcing a limit other than the one in
the config. See [Cache internals](/lattice/docs/cache-internals) for eviction
order and the on-disk layout.

## `lattice upgrade`

```text
lattice upgrade [OPTIONS] <VERSION>
```

Moves this repo to another version of Lattice and pins it. Lattice installs that
version into `.lattice/bin` and points `.lattice/bin/lattice` at the new binary.
It also writes the version to `latticeVersion` in `lattice.json`. Every later
invocation reads that pin.

**Arguments**

| Argument | Description |
| --- | --- |
| `<VERSION>` | Version to move to, such as 0.2.0, or `latest` for the newest release. Required. |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--release-latest-url` | — | `<URL>` | GitHub API | Endpoint that names the newest stable release, for `upgrade latest` |
| `--release-list-url` | — | `<URL>` | GitHub API | Endpoint listing every release, used when no release is stable yet |

Only `upgrade latest` reads either URL. `upgrade 0.2.0` reads neither. The
archive itself comes from the global `--release-base-url`.

Plus the [global flags](#global-flags) below.

```sh
lattice upgrade 0.2.0
lattice upgrade latest
lattice --release-base-url file:///srv/lattice-mirror upgrade 0.2.0
```

When the binary running `upgrade` is not the version it just pinned, `upgrade`
prints the command to run next instead of switching for you. See
[Upgrading](/lattice/docs/upgrading) for the drift nag this pin suppresses and
how every other command honors the pin.

## `lattice completions`

```text
lattice completions [OPTIONS] <SHELL>
```

Prints a shell completion script to stdout.

**Arguments**

| Argument | Description |
| --- | --- |
| `<SHELL>` | Shell to generate completions for. One of `bash`, `elvish`, `fish`, `powershell`, `zsh`. Required. |

`completions` has no flags of its own. The [global flags](#global-flags) below
still parse.

```sh
lattice completions zsh > ~/.zsh/completions/_lattice
lattice completions bash
```

## `lattice version`

```text
lattice version [OPTIONS]
```

Prints version information. Without `--json` it prints the splash. With `--json`
it prints one line of JSON.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--json` | — | — | off | Output version information as JSON |

Plus the [global flags](#global-flags) below.

```sh
lattice version
lattice version --json
```

```json
{"version":"1.0.0-beta-2","target":"aarch64-apple-darwin","arch":"aarch64"}
```

## Global flags

These parse on `lattice` itself and on every subcommand. Put them before or
after the subcommand name.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--loquacious` | `-l` | — | off | Print raw `workspace:task:` lines instead of the live display |
| `--verbose` | `-v` | — | off | Hidden alias for `--loquacious` |
| `--no-version-check` | — | — | off | Run this binary even when the repo pins another version |
| `--theme` | — | `light` \| `dark` | detected | Shade the logo for a light or dark terminal |
| `--release-base-url` | — | `<URL>` | GitHub releases | Base URL to download release archives from. A `file://` base works offline |

`--theme` takes `light` or `dark` and nothing else. A third value is a parse
error rather than a fall-back to detection. Without the flag, Lattice reads
`LATTICE_THEME`, then the terminal's own `COLORFGBG`, and treats a background of
ANSI `7` or `15` as light. With no signal at all it uses the dark shade.

`--release-base-url` is global because `upgrade` is not the only command that
downloads. An invocation in a repo pinning a version that is not installed
fetches it too, under whatever command you typed.

`--verbose` and `-v` spell one hidden alias for `--loquacious`. Neither appears
in any `--help` output.

`-h` and `--help` work on `lattice` and on every subcommand. `-V` and
`--version` print the compiled-in binary version and exist only on `lattice`
itself, so `lattice run -V` is a parse error.

## Option precedence

Where a setting can come from more than one place, Lattice resolves it in this
order, highest first.

| Source | Examples |
| --- | --- |
| CLI flag | `-l`, `--no-version-check`, `--theme`, `--release-base-url`, `--max-size` |
| Environment variable | `LATTICE_NO_VERSION_CHECK`, `LATTICE_THEME`, `LATTICE_RELEASE_BASE_URL` |
| `settings` in `lattice.json` | `settings.loquacious`, `settings.versionCheck`, `settings.maxCacheSize`, `settings.cacheDir` |
| Built-in default | — |

Not every setting has all four sources. `--loquacious` has a flag and
`settings.loquacious`, and no variable of its own. `--theme` has a flag and
`LATTICE_THEME`, and no `settings` entry.

Raw output has three triggers outside this order, each sufficient on its own:
`CI` set to any value, stdout that is not a terminal, and a persistent task in
the graph. See [Environment
variables](/lattice/docs/environment-variables) for every variable Lattice
reads.

## Exit codes

| Exit code | Meaning |
| --- | --- |
| `0` | Success. Also a `run` whose filter matched no workspace, and a `run` against an empty `workspaces` array |
| `1` | Any error Lattice raises: a missing `lattice.json`, an unknown task name, a failed task, an unset cache limit on `prune`, or any other failure |
| `2` | The command line was rejected before Lattice ran anything: an unknown subcommand, an unrecognized flag, a bad value for `--theme` or `<SHELL>`, or a missing required argument |
| `130` | A `lattice run` was interrupted by Ctrl-C or `SIGTERM` |

A failing task exits `1` whether the run stopped at the first failure or kept
going under `--continue`.

Without `--continue`, the first failing task is the process's error and no
further tasks start, though any already in flight run to completion:

```text
Error: task 'app:build' failed, stopping the run
```

With `--continue`, independent branches keep running and anything downstream of
a failure is skipped rather than started. No separate error line prints in that
case. The run summary reports the counts and the process exits `1`:

```text
lattice: 2 tasks, 0 cached, 2 failed, 0.01s
```

`--sequentially` applies the same rule per phase. A failing phase stops the
remaining phases unless `--continue` is also set, in which case every phase runs
and the process exits `1` if any task failed.

`130` is the shell's convention for a process ended by `SIGINT`, 128 + 2. It is
distinct from `1` on purpose, so that a CI runner cancelling a job does not read
as a build that broke. An interrupted run prints its summary and exits `130`
with no error line.

On unix, every running task's process group is sent `SIGTERM` on the way out,
given five seconds, and then killed, so a task that shelled out to a compiler or
a server takes the whole tree with it. Each task is spawned into its own process
group, which is what makes that possible and is also why the terminal's own
Ctrl-C never reaches it directly. On Windows tasks stay attached to the console,
which delivers the event to them.

For the full text of every message a non-zero exit prints, see
[Errors](/lattice/docs/errors).
