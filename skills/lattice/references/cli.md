# CLI reference

Every subcommand, flag, and default. `lattice <command> --help` is authoritative
if anything here disagrees with the installed binary.

Bare `lattice` with no subcommand prints the splash and exits `0`. There is no
"missing subcommand" error.

An error prints as one `error:` line with each cause indented under it, and the
command exits `1`:

```text
error: workspace 'web' cannot run
  engine 'node' is not installed, or not on PATH
    wanted   >=22
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

## `lattice run [OPTIONS] <TASKS>... [-- <ARGS>...]`

Runs one or more tasks in every workspace that has them, in dependency order.
A task name does not have to appear in `lattice.json`: any task a workspace's
own files define, or its tool's verbs include, can be run. A name no workspace
has fails with the list of names that exist:

```text
error: no workspace has a 'tset' task. Did you mean 'test'?
Tasks: build, check, clean, dev, format, lint, test
```

Stacked names merge into one graph, so a dependency they share runs once.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--sequentially` | `-s` | — | off | One graph per named task, each run to completion in the order given |
| `--filter` | `-f` | `<PATTERN>` | none | The workspaces whose name contains this substring, plus everything they depend on. Replaces the working-directory scope |
| `--concurrency` | — | `<N>` | logical CPUs | Cap how many tasks run at once. `0` means the default, which is `4` when the CPU count cannot be read |
| `--continue` | — | — | off | Keep starting tasks that don't depend on the failure. Still exits `1` |
| `--no-cache` | — | — | off | No lookup and no store for this run |
| `--force` | — | — | off | Skip the lookup but still store, replacing the entry |
| `--dry-run` | — | — | off | Print the resolved graph and every command, run nothing |
| `-- <ARGS>` | — | — | none | Passed to the tasks named on the command line |

**The working directory scopes the run.** Run from inside a workspace (any
directory below it) and the run covers that workspace and everything it depends
on, the same closure `--filter` produces. The live display's header says `· in
web, the workspace you are in`; the stream prints `lattice: running in web, the
workspace you are in`. Run from the root, or pass `--filter`, to widen it.

**Arguments after `--`** are appended, shell-quoted, to the commands of the
named tasks in the workspaces the run is for. A prerequisite pulled in as a
dependency does not get them. For a script run through `npm run`, Lattice puts
npm's own `--` in front, so `lattice run test -- --watch=false` runs `npm run
test -- --watch=false`, and does the same for a Taskfile task;
every other command gets them directly, so Cargo's test harness needs
`lattice run test -- -- --nocapture`. Arguments are
part of the command, so they are part of the cache key.

A filter that matches no workspace, and a repo with no workspaces at all, print
a line and exit `0`:

```text
lattice: no workspaces matched filter 'apu'.
lattice: found no workspaces. The repo root names none in a workspace file and holds no project Lattice recognizes. List workspace directories under `workspaces` in lattice.json, like "apps/*".
```

A filter's matches are the roots of the run; the graph also holds everything
they depend on, and `--dry-run` tags those nodes `(dependency)`:

```text
❖ lattice  dry run · test
  → lattice-config:build (dependency)  cargo build
  → dagger:build  cargo build
  → dagger:test  cargo test
```

`--dry-run` returns before any toolchain is checked or installed.

Before the graph runs, Lattice says what it skipped. A workspace declaring a
name one typo from a requested task is a warning on every run:

```text
lattice: warning: web has no 'build' task. Did you mean 'biuld'?
```

The workspaces that simply lack the task are a note (`no 'lint' task in web,
docs, so they sit this one out`), shown on the live display and with `-v` in the
stream. A manifest that will not parse is a warning naming the file:
`web: package.json could not be parsed: ..., so it runs none of its scripts`.

When every task the run scheduled came back from cache, the live display ends on
`❖❖❖ FULL POWER` and the stream on `lattice: full power, nothing to run`. It
needs at least one task, no failures, and nothing executed.

## What a run prints

Two modes. The **live display** is used on a terminal. The **stream** is used
when stdout is not a terminal, when `CI` is set to anything, with `--stream`,
and for any run that includes a persistent task.

### The live display

```text
❖ lattice  build  · 4 workspaces

  ● ui    cached · 2.10s saved
  ✓ core  1.84s
  ✗ api   failed (code 1) 3.02s
    │ error[E0425]: cannot find value `x` in this scope
    │ error: could not compile `api`

  ⠹ web   4.10s
    │ vite v8 building for production...
    │ transforming (212) src/routes/index.tsx
  · docs  waiting on web

  3/5 · 1 cached · 1 failed
```

Above the blank line, settled tasks, one line each, printed permanently as they
finish: `✓` ran and passed, `●` came from cache (with the task time it saved),
`✗` failed (its exit code and duration, then its whole output under a red `│`),
`○` skipped or a persistent task that exited. Below, the region that redraws in
place: each running task with a spinner and its elapsed time and its last three
non-empty output lines; up to three waiting tasks with what they still wait on
(`queued` when nothing), then `+N more waiting`; and a progress count. Rows are
labeled by workspace, or `workspace:task` when the run names more than one
task. The summary replaces the region:

```text
❖  5 tasks · 1 cached · 1 failed  6.20s · 2.10s saved
```

Notes print dim as they arrive (`· using node 22.11.0 · go 1.23.0` right
under the header); warnings print as `! <message>`. `-v` adds each miss reason to the
task's settled line and prints per-task notes.

### The stream

```text
lattice: running in web, the workspace you are in
ui:build: cache hit [65dfcce1]
core:build: running
core:build:    Compiling core v0.1.0
core:build: done (1.84s)
api:build: running
api:build: error[E0425]: cannot find value `x` in this scope
api:build: FAILED (code 1) after 3.02s
lattice: 3 tasks, 1 cached, 1 failed, 3.10s, 2.10s saved
error: task 'api:build' failed, stopping the run
```

One line per event under a `workspace:task:` label: `running`, `done
(<time>)`, `cache hit [<first 8 of the key>]`, `FAILED (code <n>) after
<time>`, `EXITED (code <n>) after <time>` for a persistent task, and every line
the task prints, stdout to stdout and stderr to stderr, as it arrives. A failed
task's output is not printed a second time; a task that failed before it
printed anything (its key would not compute, its shell would not spawn) gets its
error printed under the label. A failure line drops what it does not have: a
task killed by a signal or its `timeout` reads `FAILED after 30.00s`, one that
never started reads `FAILED`. A timeout is reported as a warning just before
the `FAILED` line — `lattice: warning: app:slow: timed out after 30s and was
stopped` — so it shows even when the task had already streamed output. The
live display prints the same text as a `!` line.

`-v` adds the trace:

```text
lattice: running `build` across 3 workspaces
lattice: api:build: hash 805fa504d4ac630d
lattice: api:build: cache miss: inputs changed
```

plus every note (`lattice: using node 22.11.0`), and `<ws>:<task>: skipped
(dependency failed)` lines under `--continue`. Warnings always print as
`lattice: warning: <message>`.

Labels are colored one hue per task on a terminal, and bare when piped,
redirected, or under `CI`. `NO_COLOR` turns color off everywhere.

The summary ends with the task time hits skipped, omitted when zero. Each hit
contributes the duration its entry recorded, so it is task time, not wall clock:
four cached one-minute tasks that would have run in parallel read as `4m 00s
saved`.

## `lattice setup [OPTIONS] [WORKSPACES]...`

Provisions the toolchains under `engines` (root first, then each workspace's,
checked from inside the workspace), then installs each workspace's dependencies
with its driver's installer. Name workspaces to limit it; a name that is not a
workspace is refused: `there is no workspace named 'wbe'. Workspaces: api, web`.

| Flag | Default | Description |
| --- | --- | --- |
| `--force` | off | Reinstall even when no lockfile changed |

| Driver | Installer |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun` | `npm install`, `pnpm install`, `yarn install`, `bun install` |
| `deno` | `deno cache .` |
| `cargo` | `cargo fetch` |
| `go` | `go mod download` |
| `uv`, `poetry`, `pdm`, `pipenv`, `pip` | `uv sync`, `poetry install`, `pdm install`, `pipenv install`, `pip install -r requirements.txt` |
| `bundler` | `bundle install` |
| `gradle` | `<wrapper> dependencies` (`./gradlew`, `../gradlew`, or `gradle` when there is none) |
| `maven` | `<wrapper> dependency:resolve` (`./mvnw`, `../mvnw`, or `mvn`) |
| `dotnet`, `nuget` | `dotnet restore`, `nuget restore` |
| `pod`, `swift` | `pod install`, `swift package resolve` |
| `composer`, `mix`, `dart` | `composer install`, `mix deps.get`, `dart pub get` |
| `stack`, `cabal` | `stack build --only-dependencies`, `cabal build --only-download` |

The installer runs in the nearest directory at or above the workspace that
holds a lockfile, so the members of one npm, pnpm, yarn or Cargo tree share a
single install. It is skipped, printing `dependencies up to date`, when no
lockfile in the workspace or above it is newer than the workspace's marker under
`.lattice/setup` (one file per workspace, named for its path with `/` written
`%2F`; the root is `.marker`). The installer's output streams indented; it gets
no stdin, so an installer that prompts fails with its own message.

A workspace with no driver and no engines is skipped; with engines but no driver
it reports `toolchains ready. This workspace has no package manager to
install`; a driver with no installer reports `no dependency installer known for
'<tool>'. Skipping this workspace`.

## `lattice init [OPTIONS]`

Writes a `lattice.json` holding only what discovery would not find, plus
`.lattice/schema.json` and the `.gitignore` lines for `.lattice/cache/`,
`.lattice/toolchains/`, `.lattice/bin/`, `.lattice/setup/`, and
`.lattice-setup-marker`.

| Flag | Short | Default | Description |
| --- | --- | --- | --- |
| `--force` | — | off | Overwrite an existing `lattice.json` |
| `--yes` | `-y` | off | Write the proposal without prompting |

What it proposes:

- **Extra workspaces.** The outermost directories holding a recognized project
  that the root's workspace files do not name. A directory inside another
  project (a Gradle subproject, a fixture, a crate inside an app) is left to
  that project. When every project under a parent is chosen, they are written
  as one glob (`services/*`). The root itself is offered unchecked when there
  are other projects and no workspace file, and not at all when it is one Gradle,
  Maven or .NET build.
- **Engines.** Tool versions the root already pins, for well-known engine names:
  `.tool-versions`, `.nvmrc`, `rust-toolchain.toml`/`rust-toolchain`,
  `.python-version`, `.ruby-version`, `.java-version`, `package.json`
  `packageManager` and `engines`, a `toolchain` or `go` line in `go.mod`.
- **Task settings.** A `turbo.json`'s per-task `dependsOn`, `inputs`,
  `outputs`, `ignore` (from `!` inputs), `env`, `persistent` and `cache`, and
  its `globalDependencies` and `globalEnv`, when the directory holding it is a
  proposed workspace or the repo root, a monorepo root included. Package-scoped `web#build` entries
  and dependencies on tasks that did not come over are dropped.

Tasks a file only names are not written: they run by name as they are.

On a terminal without `--yes` it shows the workspace-file members found, then
the extra workspaces and the pins as checklists. With `--yes`, or with stdout
not a terminal, it writes the proposal without asking. Then it prints every
workspace discovery now finds:

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

`updated .gitignore` appears only when a line was missing. It refuses an
existing `lattice.json` without `--force`, and a 1.x one with a pointer to
`lattice migrate`.

## `lattice migrate [--dry-run]`

Rewrites a 1.x `lattice.json` for this version, keeping every workspace's name,
every ordering, and every command. It runs discovery against the repo and
writes down only where the result would differ from what the 1.x config said:

| 1.x said | Written as |
| --- | --- |
| A workspace no workspace file names | Its path in `workspaces` |
| A workspace file member the config left out | Its path in `exclude` |
| A name that is not the manifest's | `overrides.<path>.name` |
| A `dependsOn` no manifest states | `overrides.<name>.dependsOn` |
| A `dependsOn` a manifest states | Dropped |
| `scripts` | `overrides.<name>.tasks`, as command strings |
| `auto: false` | `"driver": "none"` |
| A driver chosen through a `>=0.0.0` engine | `overrides.<name>.driver`, and the engine dropped |
| Workspace `engines` | `overrides.<name>.engines` |
| `tasks`, `engines`, `globalDependencies`, `globalEnv`, `settings` | Kept as they are |

`latticeVersion` is set to the running version. Before writing, `migrate` parses
the result and discovers the repo with it, so what it writes loads. It prints a
line for each thing it dropped:

```text
✓ rewrote lattice.json 64 lines → 21
· dropped the `pnpm` engine: its constraint matched any version, so it only chose a driver, and drivers are chosen with `driver` now
· web: dropped dependsOn ui — its manifest already says so
```

`--dry-run` prints the new file to stdout and the notes to stderr. On a config
that is already current: `lattice.json is already written for this version.
Nothing to do.` `migrate` always runs as the binary invoked, never handed to a
pinned `latticeVersion`.

## `lattice env [OPTIONS]`

Prints the `PATH` a task would see, as a command for your shell to evaluate:
installed toolchain `bin` directories first, then (for a workspace) the
dependency bin directories from the workspace up to the root. Engines are
checked and installed first, exactly as a run would.

| Flag | Short | Argument | Default | Description |
| --- | --- | --- | --- | --- |
| `--workspace` | `-w` | `<NAME>` | the workspace you are in, else the root's engines | Whose `PATH` to print |
| `--shell` | — | `posix`, `fish`, `powershell`, `cmd` | from `$SHELL` (`fish` if it ends in fish, else `posix`; without it on Windows, `powershell` with `PSModulePath`, else `cmd`) | Which syntax |

```text
$ lattice env --shell posix
export PATH='/repo/.lattice/toolchains/node/22.11.0-1a2b3c4d/bin':'/repo/node_modules/.bin':"$PATH"
$ lattice env --shell fish
set -gx PATH '/repo/.lattice/toolchains/node/22.11.0-1a2b3c4d/bin' '/repo/node_modules/.bin' $PATH
```

Use `eval "$(lattice env)"` or `lattice env --shell fish | source`. With nothing
to add it prints nothing to stdout and says why on stderr: `lattice: nothing to
add to PATH. No engine here has an installCmd, and no package manager has
installed tools for this workspace yet`. A validate-only engine adds nothing
(the host tool is already on `PATH`) but still has to pass its check.

## `lattice prune [OPTIONS]`

Evicts cache entries least-recently-used first until the store is at or under a
size limit.

| Flag | Argument | Default | Description |
| --- | --- | --- | --- |
| `--max-size` | `<SIZE>` | `settings.maxCacheSize` | Upper bound, such as `10GB` |

With neither set, it errors: `no cache size limit set. Pass --max-size, or set
settings.maxCacheSize in lattice.json`. With `settings.maxCacheSize` set, every
run already holds the cache to it. `prune` also reclaims what nothing can read —
an artifact left without metadata, a staging file, unparseable metadata — once
it is an hour old, so a `prune` straight after an interrupted run removes none
of those. It deletes only `*.tar.gz`, `*.meta.json` and `*.tmp` files in the
cache directory.

## `lattice stats`

Reports what the cache has saved, from the run ledger. No flags.

```text
❖ lattice  stats  since 2026-05-02

  saved      3h 41m of task time
  runs       412 · 2,904 of 3,390 tasks cached (86%)
  cache      1.4 GB · 892 entries
  last 7d    27m 04s saved across 38 runs
```

With no runs recorded: `No runs recorded yet. Run a task and this fills in —
every run appends one line.` The ledger is `stats.jsonl` in the cache directory,
one JSON line per run (`{"at":...,"total":6,"cached":6,"failed":0,"savedMs":6113,"elapsedMs":5}`),
appended only when the run could store and scheduled a task (`--no-cache`
records nothing, `--force` does). Per-machine, untouched by `prune`, cleared
with the cache.

## `lattice upgrade <VERSION>`

Installs another Lattice version under `.lattice/bin`, repoints
`.lattice/bin/lattice`, and writes `latticeVersion`. `<VERSION>` is a version
(`0.2.0`) or `latest`.

| Flag | Argument | Description |
| --- | --- | --- |
| `--release-latest-url` | `<URL>` | Endpoint naming the newest stable release, for `upgrade latest` |
| `--release-list-url` | `<URL>` | Endpoint listing every release, used when none is stable yet |

## `lattice completions <SHELL>`

Prints a completion script: `bash`, `elvish`, `fish`, `powershell`, `zsh`.

## `lattice version [--json]`

Prints the splash, or with `--json`:

```json
{"version":"2.0.0","target":"aarch64-apple-darwin","arch":"aarch64"}
```

`upgrade`, `version`, `completions` and `migrate` run as the binary invoked.
Every other command hands over to a pinned `latticeVersion` when the binary is
one Lattice installed under `.lattice/bin`.

## Global flags

Parsed on `lattice` and on every subcommand, before or after its name.

| Flag | Short | Description |
| --- | --- | --- |
| `--verbose` | `-v` | Add the trace: each task's hash and miss reason, and every note |
| `--loquacious` | `-l` | Hidden alias for `--verbose` |
| `--stream` | — | Print the plain `workspace:task` stream on a terminal instead of the live display |
| `--no-version-check` | — | Run this binary even when the repo pins another version |
| `--theme <THEME>` | — | `light` or `dark`: shades the logo |
| `--release-base-url <URL>` | — | Where release archives download from. A `file://` base works offline |

`-V`/`--version` exist only on `lattice` itself.

## Exit codes

| Code | Meaning |
| --- | --- |
| `0` | Success, including a filter that matched nothing and a repo with no workspaces |
| `1` | Any error Lattice raises, including a failed task |
| `2` | clap rejected the command line |
| `130` | Ctrl-C or `SIGTERM`; running tasks were stopped. They did not fail |

On an interrupt every task's process group gets `SIGTERM`, five seconds, then
`SIGKILL`. A task stopped that way is not counted as failed, so read the exit
code rather than `0 failed`. Without `--continue`, the first failure prints
`error: task '<ws>:<task>' failed, stopping the run` and nothing new starts;
tasks in flight finish. A persistent task exiting non-zero fails the run too.

## What a task runs in

A task's command goes to the platform shell (`sh -c` on Unix, `cmd /C` on
Windows) and inherits the full environment, with:

- **An empty stdin and no controlling terminal.** Each task starts in its own
  session. A tool that reads the terminal or prompts through `/dev/tty` gets
  EOF or an error and says so instead of waiting forever.
- **`PATH`** with each installed toolchain's `bin` prepended, then the
  project's dependency bin directories, nearest first, from the workspace up to
  the root:

  | Directory | Installed by |
  | --- | --- |
  | `node_modules/.bin` | npm, pnpm, yarn, bun |
  | `vendor/bin` | composer |
  | `.venv/bin`, `.venv/Scripts` | uv, poetry, pdm, pipenv, `python -m venv` |
  | `venv/bin`, `venv/Scripts` | the same, under the other conventional name |

  Only existing directories are added, and none is part of the cache key.
- **Each name in the task's `env`**, with the value read when the key was
  computed.
- **`LATTICE_TOOLCHAIN_DIR`**, only while an `installCmd` runs: the empty
  directory to install into, also substituted literally into the command.

The working directory is the workspace.

## Environment variables Lattice reads

| Variable | Flag to prefer | Effect |
| --- | --- | --- |
| `CI` | `--stream` | Present with any value: the stream instead of the live display |
| `NO_COLOR` | — | Present: no ANSI color |
| `LATTICE_NO_VERSION_CHECK` | `--no-version-check` | Present: no nag and no handover to a pinned version |
| `LATTICE_THEME` | `--theme` | `light` or `dark` |
| `COLORFGBG` | `--theme` | Fallback: a last field of `7` or `15` means a light terminal |
| `LATTICE_RELEASE_BASE_URL` | `--release-base-url` | Where archives download from |
| `LATTICE_RELEASE_LATEST_URL` | `--release-latest-url` | Resolves `upgrade latest` |
| `LATTICE_RELEASE_LIST_URL` | `--release-list-url` | Fallback release list |
| `LATTICE_SWITCHED_FROM` | — | Internal: set on the process a version handover starts |

Precedence, highest first: the flag, the environment variable,
`settings.loquacious`/`settings.versionCheck` in `lattice.json`, the default. A
blank URL variable counts as unset.
