# CLI reference

Every subcommand, flag, and default. `lattice <command> --help` is authoritative
if anything here disagrees with the installed binary.

Bare `lattice` with no subcommand prints the splash and exits `0` — there is no
"missing subcommand" error.

## `lattice run [OPTIONS] <TASKS>...`

Runs one or more tasks across workspaces in dependency order. Each name must be
a key under `tasks` in the nearest `lattice.json`. Stacked names merge into one
graph, so a dependency they share runs once.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--sequentially` | `-s` | — | off | One graph per named task, each run to completion in the order given, instead of one merged graph |
| `--filter` | `-f` | `<PATTERN>` | none | The workspaces whose `name` contains this substring, plus everything they depend on. Accepted once per run |
| `--concurrency` | — | `<N>` | logical CPUs | Cap how many tasks run at once. `0` is ignored |
| `--continue` | — | — | off | Keep starting tasks that don't transitively depend on the failure |
| `--no-cache` | — | — | off | No lookup and no store for this run |
| `--force` | — | — | off | Alias for `--no-cache`, not a stricter mode |
| `--dry-run` | — | — | off | Print the resolved graph and every command, run nothing |

A filter that matches no workspace, and a repo with an empty `workspaces` array,
print a message and exit `0`. An unrecognized task name is an error that lists
the tasks that do exist.

A filter's matches are the roots of the run: the graph also holds the transitive
dependency closure of those workspaces, and `--dry-run` prints those nodes with a
trailing `(dependency)`. A workspace pulled in that way is only asked for the
tasks its dependents need, so `auto: false` with no script for the task you named
does not halt the run unless the filter matched that workspace.

`--dry-run` returns before any toolchain is provisioned or validated, so it shows
commands as written — an engine failure only surfaces on a real run or under
`lattice setup`.

When every task the run scheduled came back from cache, a `FULL CACHE` line
follows the summary (`lattice: full cache — nothing to run` in plain output).
It requires at least one task, zero failures, and zero tasks that actually
executed, so a filter matching nothing does not print it.

## `lattice setup [OPTIONS] [WORKSPACES]...`

Provisions the toolchains declared under `engines` (root first, so dependency
installers see the pinned `PATH`), then runs each workspace's native dependency
installer — one per detected driver. Pass workspace names to limit it; omit them
for all. A repo with no workspaces still has its root `engines` provisioned.

| Flag | Default | Description |
| --- | --- | --- |
| `--force` | off | Reinstall even when the lockfile hasn't changed |

A workspace's install step is skipped when its lockfile is no newer than the
last successful install (tracked by a `.lattice-setup-marker` file in the
workspace). A workspace with no recognized driver and no `engines` is skipped
silently.

## `lattice init [OPTIONS]`

Scaffolds `lattice.json`, a committed `.lattice/schema.json`, and the
`.gitignore` lines that keep `.lattice/cache/`, `.lattice/toolchains/`, and
`.lattice/bin/` out of version control. Existing `.gitignore` content is left
alone.

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--force` | — | off | Overwrite an existing `lattice.json` |
| `--yes` | `-y` | off | Write what the scan finds without prompting |

`init` scans the repo before writing. Every directory holding a recognized
manifest (`package.json`, `Cargo.toml`, `go.mod`, `pyproject.toml`, `Gemfile`,
`pom.xml`, `build.gradle`, `composer.json`, `mix.exs`, a `.csproj`, …) is
proposed as a workspace, and every tool version already recorded in
`.tool-versions`, `.nvmrc`, `rust-toolchain.toml`, `.python-version`,
`.ruby-version`, `.java-version`, `package.json` (`packageManager`, `engines`),
or `go.mod` is proposed as an engine. The walk skips hidden directories,
gitignored paths, and dependency/output trees (`node_modules`, `target`,
`dist`, `build`, `out`, `vendor`, …).

On an interactive terminal with neither `--yes` nor piped stdin, `init` shows
both lists pre-checked so you can uncheck what's wrong. A repo root holding only
a workspace declaration is offered alongside its members but starts unchecked.
If the scan finds nothing, or you uncheck everything, `init` requires at least
one workspace or engine before it writes — it never produces a config that does
nothing.

Without a TTY it behaves as `-y` and never blocks a pipeline on a prompt; since
there is no one to ask, a scan that finds nothing writes the bare skeleton
instead. Running against an existing `lattice.json` without `--force` is an
error.

A candidate whose driver stays ambiguous (a bare `Cargo.toml` whose lockfile is
at the repo root, a lone `pom.xml`) is offered but starts unchecked, so `-y`
leaves it out: declaring it would halt the next run. `init` names those
directories on the way out. Add them by declaring a driver in their `engines`,
then adding the workspace.

## `lattice prune [OPTIONS]`

Evicts cache artifacts least-recently-used first until the store is at or under
a size limit. Every cache hit refreshes an entry's last-used time.

| Flag | Argument | Default | Description |
| --- | --- | --- | --- |
| `--max-size` | `<SIZE>` | `settings.maxCacheSize` | Upper bound, e.g. `10GB` |

A size is an integer plus `B`, `KB`, `MB`, `GB`, or `TB` (case-insensitive, base
1024), or a bare integer of bytes. With neither `--max-size` nor
`settings.maxCacheSize` set, `prune` errors rather than guess a limit.

## `lattice upgrade <VERSION>`

Installs another Lattice version under `.lattice/bin`, repoints
`.lattice/bin/lattice` at it, and writes it to `latticeVersion` in
`lattice.json`. Commit that change and everyone on the repo moves together.
`<VERSION>` is a version (`0.2.0`) or `latest`.

| Flag | Argument | Description |
| --- | --- | --- |
| `--release-latest-url` | `<URL>` | Endpoint naming the newest stable release, for `upgrade latest` |
| `--release-list-url` | `<URL>` | Endpoint listing every release, used when no release is stable yet |

Both only matter to `upgrade latest`; a plain `upgrade 0.2.0` never asks either
endpoint anything. The archive itself comes from `--release-base-url`, which is
global because the version handover downloads too.

If the binary running `upgrade` is not the version it just pinned, it prints the
command to run next rather than switching for you.

## `lattice completions <SHELL>`

Prints a completion script to stdout. `<SHELL>` is one of `bash`, `elvish`,
`fish`, `powershell`, `zsh`. No flags of its own.

## `lattice version [--json]`

Prints the splash, or a single-line JSON object with `--json`:

```json
{"version":"1.0.0-beta-2","target":"aarch64-apple-darwin","arch":"aarch64"}
```

`version` and `completions` answer for the binary that was invoked and are never
handed off to a pinned version.

## Global flags

Declared `global` in clap, so they parse on `lattice` itself and on every
subcommand, before or after the subcommand name.

| Flag | Short | Description |
| --- | --- | --- |
| `--loquacious` | `-l` | Stream the plain line-by-line log instead of the interactive display |
| `--verbose` | `-v` | Hidden alias for `--loquacious` |
| `--no-version-check` | — | Run this binary even when the repo pins another version |
| `--theme <THEME>` | — | `light` or `dark`; tunes the splash art's teal shade. Any other value is a parse error |
| `--release-base-url <URL>` | — | Base URL to download release archives from. A `file://` base works offline |

`-V`/`--version` prints the compiled-in binary version but exists only on
`lattice` itself; `lattice run -V` is a parse error.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success, including a filter that matched nothing and an empty `workspaces` array |
| `1` | Any error Lattice raises: missing `lattice.json`, unknown task name, failed task, unset cache limit on `prune`, and so on |
| `2` | clap rejected the command line before Lattice ran anything |

A failing task always exits `1`, with or without `--continue`. Without
`--continue`, the first failing task is printed as
`task '<workspace>:<task>' failed, stopping pipeline` and nothing further
starts, though tasks already in flight run to completion. With `--continue`, the
run summary (task/cache/fail counts plus tasks skipped because a prerequisite
failed) is what's printed. `--sequentially` applies the same rule per phase.

A `persistent: true` task that exits non-zero also counts in that summary and
exits `1`, whether or not `--continue` was passed. It is found after the graph
has drained, so it stops nothing.

## Environment variables Lattice reads

Prefer the flag wherever one exists — it wins over the variable, and it is what
the CLI is built around. The variables are kept so an exported value keeps
working.

| Variable | Flag to prefer | Effect | Counts as set when |
| --- | --- | --- | --- |
| `CI` | `-l` | Forces plain output | Present, any value, including empty |
| `NO_COLOR` | — | Disables ANSI color. Nothing turns it back on | Present, any value |
| `LATTICE_NO_VERSION_CHECK` | `--no-version-check` | Suppresses the drift nag and the handover to a pinned `latticeVersion` | Present, any value |
| `LATTICE_THEME` | `--theme` | `light` or `dark`; forces the splash's teal shade | Recognized value present |
| `COLORFGBG` | `--theme` | Splash theme fallback when neither the flag nor `LATTICE_THEME` is set. Read as `fg;bg`; a trailing `7` or `15` means a light background | It parses |
| `LATTICE_RELEASE_BASE_URL` | `--release-base-url` | Base URL `upgrade` and the version switch download archives from | Present and not whitespace-only |
| `LATTICE_RELEASE_LATEST_URL` | `--release-latest-url` | Endpoint resolving `upgrade latest` to the newest stable release | Present and not whitespace-only |
| `LATTICE_RELEASE_LIST_URL` | `--release-list-url` | Fallback when the latest-stable endpoint names nothing | Present and not whitespace-only |
| `LATTICE_SWITCHED_FROM` | — | Internal. Set on the process an invocation is handed to after a version switch. Not meant to be set by hand | Present, any value |

`LATTICE_SWITCHED_FROM` has no flag on purpose: the process being handed to is a
different build of Lattice, and one older than the flag would reject it. For the
same reason, a repo pinning an older version is the one case where the
`LATTICE_RELEASE_*` variables are safer than the flags — the whole command line
is passed through the handover, so a flag the pinned build does not know is a
parse error there.

`CI` and `-l` are independent triggers for the same plain output mode; there is
no way to force the interactive display back on from inside `CI=1`.

In that mode the `workspace:task` label leading each line is colored, one color
per task, so interleaved parallel output can be followed by eye. The color is
emitted only when stdout is a real terminal: piped, redirected, or under `CI`
the labels are bare, so captured output stays byte-for-byte greppable and
carries no escapes to strip. `NO_COLOR` suppresses it at a terminal too.

## What Lattice sets for a spawned command

A task's command runs through the platform shell (`sh -c` on Unix, `cmd /C` on
Windows) as a child that inherits the full environment — nothing is cleared. On
top of that:

| Variable | Shape | When |
| --- | --- | --- |
| `PATH` | Resolved toolchain bin directories prepended, in order | Every task with a provisioned engine, and every `setup` install command |
| Each name in a task's `env` | The value read when the cache key was computed | Every task declaring `env` |
| `LATTICE_TOOLCHAIN_DIR` | Absolute path to the toolchain install directory, also literal-substituted into `installCmd` | Only while running an `installCmd` |

The `PATH` change is scoped to that one child process. It never touches the
shell Lattice runs in.

## Option precedence

Highest first:

1. CLI flag — `-l`, `--no-version-check`, `--theme`, `--release-base-url`
2. Environment variable — `LATTICE_NO_VERSION_CHECK`, `LATTICE_THEME`, `LATTICE_RELEASE_BASE_URL`
3. `settings` in `lattice.json` — `settings.loquacious`, `settings.versionCheck`
4. Built-in default

For the URL overrides a blank value does not count as given at either of the
first two steps, so an inherited `LATTICE_RELEASE_BASE_URL=` falls through to
the default rather than building an empty URL.
