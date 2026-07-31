---
title: CLI reference
description: Every lattice command, every flag, exit codes, and precedence.
group: Reference
order: 2
---

# CLI reference

Every subcommand, every flag, every default. Where this page and `lattice
<command> --help` disagree, `--help` is right and this page is a docs bug.

For *why* a flag behaves the way it does, see [Selecting what
runs](/lattice/docs/filtering), [Caching](/lattice/docs/caching), [Engines and
provisioning](/lattice/docs/engines), and [Upgrading](/lattice/docs/upgrading).

## Bare `lattice`

Running `lattice` with no subcommand prints the same branded splash as
`lattice version` (an ASCII mark, the version line, and the tagline), then a
line pointing at `--help`. It exits `0`; there is no "missing subcommand"
error.

```sh
lattice
```

```text
❖ lattice  1.0.0-beta-2  (aarch64)
A high-performance, local toolchain for managing monorepos.

Run lattice --help to see available commands.
```

## `lattice run`

```text
lattice run [OPTIONS] <TASKS>...
```

Runs one or more tasks across your workspaces, in dependency order. Each task
name must exist in the `tasks` map of the nearest `lattice.json`. Stacked task
names are merged into a single execution graph, so a dependency they share
runs once.

**Arguments**

| Argument | Description |
| --- | --- |
| `<TASKS>...` | One or more tasks to run across workspaces (e.g. `lint test build`). Required — at least one. |

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--sequentially` | `-s` | — | off | Run the given tasks one at a time, each graph to completion, in order — instead of merging them into one combined graph |
| `--filter` | `-f` | `<PATTERN>` | none | Only run in workspaces whose name contains this pattern |
| `--concurrency` | — | `<N>` | number of CPUs | Cap how many tasks run at once |
| `--continue` | — | — | off | Keep running independent tasks after a failure instead of stopping |
| `--no-cache` | — | — | off | Ignore the cache and re-run every task |
| `--force` | — | — | off | Ignore the cache for this run (alias for `--no-cache`) |
| `--dry-run` | — | — | off | List the tasks that would run, then exit without running them |

Plus the [global flags](#global-flags) below.

```sh
lattice run build
lattice run lint test build
lattice run lint test build --sequentially
lattice run test --filter api
lattice run lint --concurrency 4 --continue
```

A filter that matches no workspace, or a repo with an empty `workspaces`
array, prints a message and exits `0` — it is not a failure. An unrecognized
task name is an error naming the tasks that do exist. See [Selecting what
runs](/lattice/docs/filtering) for how `--filter` and `--dry-run` interact with
the task graph, and [Persistent tasks](/lattice/docs/persistent-tasks) for how
a persistent task in the closure forces raw output regardless of `-l`.

## `lattice setup`

```text
lattice setup [OPTIONS] [WORKSPACES]...
```

Provisions the toolchains declared under `engines` (root first, so dependency
installers see the pinned `PATH`), then runs each workspace's native
dependency installer — `pnpm install`, `cargo fetch`, `poetry install`, and so
on, one per detected driver. A repo with no workspaces still has its root
`engines` provisioned.

**Arguments**

| Argument | Description |
| --- | --- |
| `[WORKSPACES]...` | Only set up specific workspaces, by name. Omit to set up all of them. |

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

Setup skips a workspace's install step when its lockfile is not newer than the
last successful install (tracked by a `.lattice-setup-marker` file in the
workspace); `--force` reinstalls regardless. A workspace with no recognized
driver and no `engines` is skipped silently.

## `lattice init`

```text
lattice init [OPTIONS]
```

Scaffolds a `lattice.json` in the current directory, along with a committed
`.lattice/schema.json` and the `.gitignore` lines that keep the cache and
provisioned toolchains out of version control. On an interactive terminal
without `--yes`, it prompts for workspaces and engines instead of writing the
bare skeleton.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--force` | — | — | off | Overwrite an existing `lattice.json` |
| `--yes` | `-y` | — | off | Accept defaults and write the skeleton without prompting |

Plus the [global flags](#global-flags) below.

```sh
lattice init
lattice init --yes
lattice init --force
```

Without a TTY (a script, CI), `init` writes the skeleton whether or not
`--yes` is passed, so it never blocks waiting on prompts. Running `init`
against an existing `lattice.json` without `--force` is an error.

## `lattice prune`

```text
lattice prune [OPTIONS]
```

Evicts cache artifacts, oldest-used first, until the local cache is at or
under a size limit.

**Flags**

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--max-size` | — | `<SIZE>` | `settings.maxCacheSize` | Upper bound on the cache size (e.g. `10GB`) |

Plus the [global flags](#global-flags) below.

```sh
lattice prune
lattice prune --max-size 5GB
```

A size accepts `B`, `KB`, `MB`, `GB`, or `TB` (case-insensitive, base 1024) or
a bare integer of bytes. If neither `--max-size` nor
`settings.maxCacheSize` is set, `prune` errors rather than guessing a limit.
See [Cache internals](/lattice/docs/cache-internals) for eviction order and
on-disk layout.

## `lattice upgrade`

```text
lattice upgrade [OPTIONS] <VERSION>
```

Moves this repo to another version of Lattice: installs it under
`.lattice/bin`, points `.lattice/bin/lattice` at it, and writes it to
`latticeVersion` in `lattice.json`. Every later invocation reads that pin, so
commit the change and everyone on the repo moves together.

**Arguments**

| Argument | Description |
| --- | --- |
| `<VERSION>` | Version to move to (e.g. `0.2.0`), or `latest` for the newest release. Required. |

**Flags**

| Flag | Argument | Default | Description |
| --- | --- | --- | --- |
| `--release-latest-url` | `<URL>` | GitHub API | Endpoint that names the newest stable release, for `upgrade latest` |
| `--release-list-url` | `<URL>` | GitHub API | Endpoint listing every release, used when no release is stable yet |

Both are consulted only by `upgrade latest`; `upgrade 0.2.0` asks neither. The
archive itself comes from the global `--release-base-url`. Plus the [global
flags](#global-flags) below.

```sh
lattice upgrade 0.2.0
lattice upgrade latest
lattice --release-base-url file:///srv/lattice-mirror upgrade 0.2.0
```

If the binary running `upgrade` is not the version it just pinned, it prints
the command to run next rather than switching for you. See
[Upgrading](/lattice/docs/upgrading) for the version-drift nag this pin
suppresses and how the pin is honored on every other command.

## `lattice completions`

```text
lattice completions [OPTIONS] <SHELL>
```

Prints a shell completion script to stdout.

**Arguments**

| Argument | Description |
| --- | --- |
| `<SHELL>` | Shell to generate completions for. One of `bash`, `elvish`, `fish`, `powershell`, `zsh`. Required. |

Plus the [global flags](#global-flags) below (`completions` has no flags of its own).

```sh
lattice completions zsh > ~/.zsh/completions/_lattice
lattice completions bash
```

## `lattice version`

```text
lattice version [OPTIONS]
```

Prints version information: the branded splash by default, or a single-line
JSON object with `--json`.

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

These parse on `lattice` itself and on every subcommand: put them before or
after the subcommand name.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--loquacious` | `-l` | — | off | Stream the plain line-by-line log instead of the interactive UI |
| `--verbose` | `-v` | — | off | Hidden alias for `--loquacious` |
| `--no-version-check` | — | — | off | Run this binary even when the repo pins another version |
| `--theme` | — | `light` \| `dark` | detected | Tune the splash art's teal shade for a light or dark terminal |
| `--release-base-url` | — | `<URL>` | GitHub releases | Base URL to download release archives from. A `file://` base works offline |

`--theme` takes `light` or `dark` and nothing else; a third value is a parse
error, not a fall-back to detection. With no flag, Lattice reads
`LATTICE_THEME`, then the terminal's own `COLORFGBG`.

`--release-base-url` is global because `upgrade` is not the only command that
downloads: an invocation in a repo pinning a version that isn't installed
fetches it too, under whatever command you typed.

`--verbose`/`-v` is a hidden alias for `--loquacious` and does not appear in
`--help`.

`-h`/`--help` works on `lattice` and on every subcommand. `-V`/`--version`
prints the compiled-in binary version (e.g. `1.0.0-beta-2`) and exists only on
`lattice` itself — `lattice run -V` is a parse error.

## Option precedence

Wherever a setting can come from more than one place, Lattice resolves it in
this order, highest first:

| Source | Examples |
| --- | --- |
| CLI flag | `-l`, `--no-version-check`, `--theme`, `--release-base-url` |
| Environment variable | `LATTICE_NO_VERSION_CHECK`, `LATTICE_THEME`, `LATTICE_RELEASE_BASE_URL` |
| `settings` in `lattice.json` | `settings.loquacious`, `settings.versionCheck` |
| Built-in default | — |

This holds everywhere in this reference. See [Environment
variables](/lattice/docs/environment-variables) for the full list of variables
Lattice reads.

## Exit codes

| Exit code | Meaning |
| --- | --- |
| `0` | Success — including a `run` whose filter matched no workspace, and a `run` against an empty `workspaces` array |
| `1` | Any error Lattice itself raises: a missing `lattice.json`, an unrecognized task name, a failed task, an unset cache limit on `prune`, or any other failure |
| `2` | The command line itself was rejected before Lattice ran anything: an unknown subcommand, an unrecognized flag, or a missing required argument |

A failing task exits `1` whether the run stopped at the first failure or kept
going with `--continue`.

Without `--continue`, the first failing task is printed as the error (`task
'<workspace>:<task>' failed, stopping pipeline`) and no further tasks start,
though any already in flight run to completion. With `--continue`, the run
summary line reports it instead — task, cache, and fail counts, plus any
downstream tasks skipped because a prerequisite failed — with no separate error
line. `--sequentially` applies the same rule per phase: a failing phase stops
the remaining phases unless `--continue` is also set, in which case every phase
runs and the process exits `1` if any failed.
