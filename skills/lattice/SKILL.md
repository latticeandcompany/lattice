---
name: lattice
description: >
  Reference for Lattice, a CLI that runs and caches a monorepo's tasks in any
  language and pins the versions of the tools those tasks use. It reads the
  workspaces, the dependencies between them and their tasks from the repo's own
  manifests; an optional `lattice.json` at the root says only what those cannot.
  Use this skill when:
  (1) the repo contains a `lattice.json`,
  (2) the user asks to build, test, lint, or run anything through `lattice`,
  (3) adding Lattice to a repo, or adding a workspace, task setting, or engine,
  (4) a `lattice` run fails with a driver, engine, cache, config, or graph error,
  (5) a task is missing the cache or hitting it when it should not,
  (6) wiring `lattice` into CI, or moving a 1.x config to this version.
license: ISC
compatibility: lattice 2.0.0+
metadata:
  author: latticeandcompany
  version: "2.0.0"
allowed-tools: Bash(lattice:*), Read, Write, Edit, Glob, Grep
---

# Lattice

Lattice is one CLI with two halves, usable together or apart. It runs a
monorepo's tasks in dependency order and in parallel, caching each result by
content so unchanged work is skipped. It also pins the versions of the tools
those tasks use.

It does not replace the commands a repo already has. Each workspace runs its
real command — `pnpm run build`, `cargo clippy`, `./gradlew test` — found in the
workspace's own files or written out in `lattice.json`.

## Vocabulary

Use these words. Another word for the same thing produces configs and advice
that read as if they came from a different tool.

| Term | Meaning |
| --- | --- |
| repo | The git repository, with `lattice.json` at its root |
| workspace | One directory Lattice runs tasks in. The unit of task running and caching |
| task | A name like `build`, resolved to a concrete shell command in each workspace that has it |
| driver | The tool that runs a workspace's tasks: `pnpm`, `cargo`, `gradle`, `just`, … |
| engine | A versioned tool named under `engines` |
| toolchain | An engine Lattice installed under `.lattice/toolchains/` with its `installCmd` |
| override | An entry under `overrides`, applying to the workspaces its key matches |
| cache hit | A task whose stored result was verified and restored instead of running |
| saved time | Task time a run's hits skipped, summed from what each entry recorded. Not wall clock |
| persistent task | A long-running task such as a dev server |

A workspace is not a project, a package, or a module. The repo is not a
workspace unless it is one on its own.

## Orient before you act

1. Find the root. Every command walks up from the current directory to the
   nearest `lattice.json`. No config in this directory or any parent is a hard
   error.
2. Ask the binary what a task will do. The config does not list most of what
   Lattice runs:

   ```sh
   lattice run build --dry-run
   ```

   This loads the config, discovers every workspace, resolves every
   `workspace:task` node and its exact command, prints them in dependency order,
   and exits without running or caching anything. It is the cheapest way to see
   which workspaces exist, what each will run, and that an edit parses.
3. Read `lattice.json` for what it adds: extra workspace directories,
   `exclude`, task settings under `tasks`, and `overrides`.

`lattice <command> --help` is authoritative if anything here disagrees with the
installed binary.

## How Lattice finds workspaces

In order:

1. **The repo root's own workspace files.** `package.json` `workspaces` (an
   array, or `{ "packages": [...] }`), `pnpm-workspace.yaml` `packages`,
   `Cargo.toml` `[workspace] members` (minus `exclude`), `go.work` `use`,
   `pyproject.toml` `[tool.uv.workspace] members` (minus `exclude`), and
   `deno.json` or `deno.jsonc` `workspace`. A `!pattern` entry removes what the
   entries before it matched. A member has to hold that ecosystem's manifest to
   count.
2. **`workspaces` in `lattice.json`.** Directories or globs, relative to the
   root, added to whatever step 1 found. `*` does not cross a `/`; `**` does.
   `"."` makes the root a workspace.
3. **`exclude` in `lattice.json`.** Globs of workspace paths removed from both.
4. **Otherwise the root.** A repo that names no workspace anywhere is one
   project: the root is its only workspace.

Rules that follow from that:

- **Only the root's workspace files are expanded.** A directory `workspaces`
  names is one workspace however it is organised inside: a nested monorepo, a
  Cargo workspace in a subdirectory, a Gradle build.
- **A Gradle, Maven or .NET build is one workspace**, at the build's root, never
  one per subproject. Two builds of one Gradle project at once fight over its
  locks and output directories; the build tool orders its own subprojects.
  List the build root (`"jvm"`), not its subprojects (`"jvm/*"`).
- **The root is not a workspace when it names members.** A root `package.json`
  with `workspaces`, or a virtual `Cargo.toml`, only organises. The exceptions
  are a Cargo root with its own `[package]` and a uv root with its own
  `[project]`, which are members too. Add `"."` to `workspaces` to make the root
  a workspace anyway, for its own scripts.
- **A glob match with nothing Lattice recognizes is skipped silently.** A
  directory of notes under `apps/*` is not an error. A `workspaces` entry that
  matches no directory at all is.
- **Hidden, gitignored, dependency and output directories are never matched**
  by a glob (`node_modules`, `target`, `dist`, `build`, `out`, `vendor`, `venv`,
  `__pycache__`). A literal path is looked up directly, so it still works.

A workspace is named after its manifest: `package.json` `name` (the full name,
scope included: `@acme/ui`), then Cargo `[package] name`, then pyproject
`[project] name`, then `composer.json` `name`, then the directory's name. Go
modules take the directory's name. Two workspaces that come out with the same
name are both named by their path instead (`apps/web`, `sites/web`).
`overrides.<key>.name` renames one.

## How dependencies are found

Workspace-to-workspace order comes from the manifests, never from a list you
maintain:

| Ecosystem | What makes an edge |
| --- | --- |
| npm, pnpm, yarn, bun | Another workspace's package name in `dependencies`, `devDependencies`, `peerDependencies`, or `optionalDependencies`, whatever the version spec (`workspace:*`, `file:`, a range) |
| Cargo | A `path` dependency, or `workspace = true` inherited from a root `[workspace.dependencies]` entry with a `path`, in `dependencies`, `dev-dependencies`, `build-dependencies`, and their `target.*` forms |
| Go | A `require` of another workspace's module path, or a `replace` to a local directory |
| Python | Another workspace's project name (PEP 503 normalized) in `[project] dependencies`, `optional-dependencies`, or `[dependency-groups]`, or a `[tool.uv.sources]` `path` |
| Composer | Another workspace's name in `require` or `require-dev` |

An edge between languages is the one thing no manifest can say. Write it in
`overrides.<workspace>.dependsOn`. Inferred and declared edges combine.

A workspace edge changes no ordering on its own. It is what `^task` follows:
`build` depends on `^build` by default, so builds already run in edge order.

## How tasks are found

A workspace can run a task when its own files define it or its tool has the
verb. Nothing else is invented.

- **Script maps run exactly what they declare.** `package.json` `scripts`
  (`npm run x`, `pnpm run x`, `yarn run x`, `bun run x`), deno `tasks`
  (`deno task x`), `composer.json` `scripts` (`composer run-script x`), justfile
  recipes (`just x`), Taskfile tasks (`task x`), Rakefile tasks
  (`bundle exec rake x` when there is a `Gemfile`), `[tool.pdm.scripts]`
  (`pdm run x`), Pipfile `[scripts]` (`pipenv run x`), `turbo.json` tasks
  (`turbo run x`), `nx.json` `targetDefaults` (`nx run-many -t x`).
- **Build tools get the verbs they have.** `lint` in a Cargo workspace is
  `cargo clippy`; `check` in Go is `go vet ./...`. The full table is in
  `references/toolchains.md`.
- **Some verbs need the workspace to show the tool.** Python gets `test` only
  when `pytest` is a declared dependency, `lint`/`format` only with `ruff`
  (or `black`/`flake8`), `check` only with `mypy`. Gradle and Maven get
  `format`/`lint` only when the build uses Spotless. Ruby gets `test` from
  `rspec` in the Gemfile or a `Rake::TestTask`, `lint` from `rubocop`.
- **A workspace without the task sits the run out.** It drops out of the graph
  and the run carries on. `lattice run lint` in a repo where only some
  workspaces lint runs lint where it exists. A task no workspace has is an error
  that lists the tasks that do exist.
- **Any task can be written out.** `overrides.<ws>.tasks.<name>` as a command
  string, or an object whose `run` is the command, replaces or adds a task in
  that workspace.

Before `lattice run`, `--dry-run` shows the exact command each workspace will
run. Read every line when adding a task setting.

## Task defaults and layering

Every task a workspace has gets its settings from four layers, each replacing
only the fields it sets:

1. Lattice's defaults for the task name:

   | Task | Default |
   | --- | --- |
   | `build` | `dependsOn: ["^build"]` |
   | `dev`, `start`, `serve`, `watch` | `persistent: true` |
   | `clean`, `deploy`, `publish`, `release` | `cache: false` |

2. The tool's build directory as `outputs` for a `build` the workspace's own
   files define: `dist/**` for npm, pnpm, yarn, bun, deno, uv, poetry and pdm;
   `build/**` beside each Gradle build file; `target/**` beside each `pom.xml`; `**/bin/Debug/**` and
   `**/bin/Release/**` for .NET (each subproject writes its own). None for
   Cargo (its `target/` is shared), Go, or a uv workspace member (it builds
   into the root's `dist/`). A `build` written out in an override
   gets no guessed outputs.
3. The root `tasks.<name>` object.
4. The workspace's override for that task.

A list replaces rather than extends: an override that names `outputs` means
exactly those outputs. `"start": { "persistent": false }` turns a default off.

## What does not exist

Every item here is something models invent, or something 1.x had. None of it
parses.

| Not a thing | What to write instead |
| --- | --- |
| A workspace object `{ "name": ..., "path": ... }` | A path or glob string in `workspaces`; `overrides.<key>.name` to rename |
| `auto`, `scripts` on a workspace | `overrides.<ws>.tasks`, and `"driver": "none"` for no inference |
| An engine with `">=0.0.0"` to choose a driver | `overrides.<ws>.driver` |
| `run` in the root `tasks` map | `overrides.<ws>.tasks.<name>.run` |
| A `projects` or `pipeline` key | `workspaces`, `tasks` |
| `workspace#task` in `dependsOn` | `^task` for dependencies, bare `task` for this workspace |
| A glob in `dependsOn` | Nothing. Name tasks literally |
| `env` as `"NAME=value"` pairs | Variable names only: `["DATABASE_URL"]` |
| `extends` or `include` | Nothing. One config file per repo |
| `maxCacheSize` as a number | A string: `"10GB"` |
| `lattice exec`, `lattice graph`, `lattice doctor`, `--affected`, `--since` | Nothing. `lattice env` for the pinned PATH, `--dry-run` for the graph |
| Any other key at any level | Nothing. An unknown key fails the load |

A 1.x config fails to load with `lattice.json is written for Lattice 1.x ...
Run \`lattice migrate\``. Run `lattice migrate`; do not hand-translate it.

## Editing `lattice.json`

Every key is optional, and `{}` is a complete config for a repo whose workspace
files say everything.

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "2.0.0",
  "workspaces": ["services/*", "tools/gen"],
  "exclude": ["services/legacy"],
  "engines": { "node": ">=22", "go": ">=1.22" },
  "globalDependencies": ["tsconfig.base.json"],
  "tasks": {
    "test": { "dependsOn": ["build"], "timeout": "10m" }
  },
  "overrides": {
    "web": {
      "dependsOn": ["api"],
      "tasks": { "e2e": { "run": "playwright test", "cache": false } }
    },
    "tools/gen": { "name": "codegen", "driver": "none", "tasks": { "build": "make" } },
    "services/*": { "engines": { "go": ">=1.23" } }
  },
  "settings": { "maxCacheSize": "10GB" }
}
```

An `overrides` key is a **workspace name**, or a **glob over workspace paths**
when it is `"."` or holds a `/` or a glob character. A top-level directory is
matched by path as `"./tools"`. Several keys can match one workspace; they
apply in the order written, `name` and `driver` last-wins, `engines` merged per
key, `dependsOn` combined, `tasks` layered per task. A key that matches no
workspace is a load error with the nearest name offered. A name key matches the
workspace's discovered name or a name a path override gave it.

`driver` names the tool when Lattice cannot tell, or overrides what it
detected; `"none"` means the workspace runs only the commands its override
writes out. A task object in an override takes every task field plus `run`.

In a task's `dependsOn`, `^build` means `build` in each workspace this one
depends on, and a bare `build` means `build` in this workspace. Those two tokens
are the whole vocabulary. A name no workspace has is an error at plan time,
with the nearest name offered.

Every field, type, default, and validation error: `references/configuration.md`.

## Rules that will bite you

- **A persistent task blocks.** Once a run starts one, `lattice run` streams its
  output and waits. Never call one from a blocking foreground command. Four
  things end such a run: the first `Ctrl-C`, a `SIGTERM`, the exit of every
  persistent task, or the failure of any task in the run. Nothing may depend on
  a persistent task. A run that includes one prints the plain stream, never the
  live display.
- **A task never gets the terminal.** Every task runs with an empty stdin and
  no controlling terminal. A tool that prompts (`sudo`, `ssh`, an interactive
  installer) fails at once with its own error instead of waiting. A dev server
  cannot read keypresses.
- **A task can outlive its own output.** A task that leaves a process running —
  a Gradle daemon, an MSBuild node, a backgrounded watcher — hands it the pipe
  Lattice reads output through. Lattice reads for half a second past the task's
  exit and stops; `-v` notes `finished leaving a process that still holds its
  output open`. The task still succeeds and caches, and the leftover keeps
  running.
- **An interrupted run reports no failures.** `Ctrl-C` and `SIGTERM` stop every
  task's process group. Read the exit code: `130` is an interrupt, `1` a real
  failure.
- **Running inside a workspace scopes the run to it.** From `packages/web`,
  `lattice run test` runs `web`'s test plus whatever it depends on, and says so.
  `--filter` replaces the scope. Run from the root for every workspace.
- **Arguments after `--` go to the tasks you named**, in the workspaces the run
  is for, never to prerequisites pulled in: `lattice run test -- --release`
  runs `cargo test --release`. For `npm run`, Lattice inserts npm's own `--`.
  Arguments are passed as written, so Cargo's test harness still needs its own
  separator: `lattice run test -- -- --nocapture`.
- **`--filter` selects the roots of a run.** It matches workspaces whose name
  *contains* the pattern (never the path), then adds everything they depend on,
  tagged `(dependency)` under `--dry-run`. Nothing that depends *on* a match is
  included.
- **A task with no `inputs` hashes its whole workspace,** honoring `.gitignore`.
  Declared `inputs` do not honor it: only `.lattice`, `.git`, `.hg`, `.svn` and
  `.jj` are skipped, so a glob reaching into `node_modules` hashes it.
- **`inputs` cannot name a file above the workspace.** Put a shared root file in
  `globalDependencies`, and repo-wide variables in `globalEnv`. Leave one out
  and every task hits cache with an artifact built before it changed.
- **A cache hit deletes what `outputs` matched before restoring.** A default
  output counts: `dist/**` in an npm workspace's `build` is cleared on a hit.
- **A key written twice fails the load,** in `tasks`, `engines`, `overrides`,
  or an override's `tasks`. Merge by hand; there is no last-wins.
- **An unknown key fails the load**, naming the key, its container, the line and
  column, and the nearest valid field. Keep `"$schema": ".lattice/schema.json"`;
  every command that loads the config rewrites that file when it is missing or
  out of date.
- **`lattice run` installs no dependencies.** It does install a toolchain whose
  `installCmd` has not run yet. `lattice setup` provisions engines and runs each
  workspace's installer once per lockfile directory.
- **Lattice never guesses a driver.** An ambiguous workspace halts with the fix
  printed. Ask which tool the workspace uses if its files do not say. See
  [When detection halts](#when-detection-halts).
- **An npm-installed binary ignores `latticeVersion`.** Only a binary under
  `.lattice/bin` switches to the pinned version. If a run behaves like another
  release, compare `lattice version` with `latticeVersion` first.

## Running tasks

```sh
lattice run build                      # every workspace that has build, in order
lattice run lint test build            # merged into one graph; shared deps run once
lattice run lint test build -s         # one graph per task, each to completion
lattice run test --filter api          # names containing api, plus what they depend on
lattice run test -- --watch=false      # pass arguments to the named task
lattice run build --dry-run            # resolve and print, run nothing
lattice run lint test --continue       # keep going past a failure, still exit 1
lattice run build --concurrency 4      # cap parallelism; default is the CPU count
lattice run build --no-cache           # neither read nor write the cache this run
lattice run build --force              # re-run and overwrite the stored entry
lattice run build --stream             # plain workspace:task lines on a terminal
lattice run build -v                   # add hashes, miss reasons and notes
```

On a terminal a run draws a live display. Finished tasks settle into one line
each (`✓ web  1.20s`, `● ui  cached · 2.00s saved`, `✗ api  failed (code 2)
1.20s` with the task's whole output under it); below them every running task
shows its elapsed time and last three output lines, the next tasks waiting say
what they wait on, and a count tracks progress.

With no terminal, with `CI` set, or with `--stream`, the run prints the plain
stream, which is what to parse:

```text
api:build: running
api:build: go: downloading example.com/lib v1.0.0
api:build: done (2.31s)
web:build: cache hit [7ef2903e]
web:build: FAILED (code 1) after 1.02s
lattice: 3 tasks, 1 cached, 1 failed, 2.40s, 0.19s saved
```

Every line a task prints is streamed under its `workspace:task:` prefix, stderr
to stderr. A failure line drops what it does not have: a task killed by a
signal or its `timeout` reads `FAILED after 30.00s`, and one that failed before
its command started reads `FAILED`. A timeout is also a warning just before
that line: `lattice: warning: app:slow: timed out after 30s and was stopped`. `-v` adds `lattice: <ws>:<task>: hash
<key>`, a `cache miss` line naming what moved, and every note.

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Success. Also a filter that matched nothing, and a repo with no workspaces |
| `1` | Any error Lattice raises, including a failed task. Printed as `error: ...` with causes indented under it |
| `2` | The command line itself was rejected |
| `130` | Ctrl-C or `SIGTERM`. Running tasks were killed on the way out |

## Caching decisions

The cache key is a sha256 over eleven separately hashed components:

| Component | What goes in |
| --- | --- |
| `environment` | Lattice version, OS, architecture, shell, workspace name, task name |
| `command` | The resolved shell command, arguments after `--` included |
| `toolchain` | The resolved toolchain identity |
| `dependencies` | The cache key of every task this one depends on |
| `upstream` | The files of every workspace this one depends on, directly or not: their tasks' `inputs` (all files when one has none), minus their `ignore` and `outputs` |
| `patterns` | The `inputs`, `outputs`, and `ignore` lists themselves |
| `env` | The names in the task's `env`, with their resolved values |
| `globalEnv` | The names in `globalEnv`, with their resolved values |
| `inputs` | Every file `inputs` matched, minus `ignore` and `outputs`: path, executable bit, contents. A symlink contributes its target path. With no `inputs`, the whole workspace |
| `manifests` | Every recognized manifest and lockfile in the workspace, plus lockfiles at the repo root |
| `globalDependencies` | The `globalDependencies` patterns and what they match |

`timeout`, `cache`, and `persistent` are not in the key. A dependency's key
moving re-runs its dependents. A task's own `outputs` never affect its key.
`chmod +x` on a hashed file moves the key; so does re-pointing a symlink.

The fields to get right: `inputs` (what the command reads; omit it and the whole
workspace is hashed), `ignore` (subtracted from `inputs`), `outputs` (what is
archived on success and restored on a hit; a task that declares outputs and
produces none is not cached and warns `no files matched outputs [...]`), `env`
(names, not pairs; set values are exported to the task), `globalDependencies`,
`globalEnv`.

`cache: false` opts one task out; a persistent task is never cached. The cache
is `.lattice/cache` (`settings.cacheDir` moves it inside the repo) and safe to
delete. With `settings.maxCacheSize`, every run that writes evicts
least-recently-used entries down to it; otherwise `lattice prune --max-size`.

**Debugging a miss:** `-v`. Each miss names the component that moved:

```text
lattice: web:build: hash a1b2c3d4e5f6a7b8
lattice: web:build: cache miss: inputs changed
```

Two misses name none: `cache miss (nothing cached for this task yet)` and
`cache miss (the entry for this key is no longer in the cache)`.

**What the cache saved:** the summary ends with the task time hits skipped —
task time, not wall clock. `lattice stats` totals it across every recorded run.

## Pinning tool versions

An `engines` entry's *shape* selects one of three behaviors:

| You write | What happens |
| --- | --- |
| Nothing, `{}`, or an object with only `versionCmd` or `bin` | Uses whatever is on `PATH`. Checks nothing |
| A version constraint and no `installCmd` | Runs the version command on the host and fails before any task starts if it does not satisfy the constraint |
| An `installCmd` | Runs it once into `.lattice/toolchains/<engine>/<version>-<hash>/`, version-checks the result, and puts its `bin` on each task's `PATH` |

Root `engines` apply to every workspace; `overrides.<key>.engines` win per
name. The bare string form works for the 40 well-known engines; any other name
needs `versionCmd`. A `gradle` or `maven` engine with no `versionCmd` is
checked through the build's wrapper (`./gradlew --version`, run in the
workspace), because the wrapper is what the build runs.

A failed check says exactly what went wrong, one fact per line:

```text
error: workspace 'web' cannot run
  engine 'node' does not match the version this repo pins
    wanted   >=22
    found    20.11.1 at /usr/local/bin/node
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

Other headlines: `is not installed, or not on PATH`, `could not be checked: its
version command failed`, `could not be installed: its installCmd exited with
code 7` (with the command's output), `installed a version that does not match
its constraint`. A run notes the versions it uses (`using node 22.11.0 · go
1.23.0`) — on the live display always, in the stream with `-v`.

A task's `PATH` also gets the project's own dependency bin directories
(`node_modules/.bin`, `vendor/bin`, `.venv/bin`, …) from the workspace up to the
root, after the toolchains. `lattice env` prints that `PATH` for your own shell:
`eval "$(lattice env)"`.

The engine list, the driver table, and the `installCmd` contract:
`references/toolchains.md`.

## When detection halts

```text
error: Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

Detection reads, in order: evidence in the workspace (a `packageManager` field,
`.tool-versions`, `mise.toml`, a wrapper, a `[tool.poetry]` table, a lockfile
only one tool writes), resolved by role (task runner over package manager over
build tool) and then by how deliberate it is; for a `package.json` or
`pyproject.toml` workspace with none of that, the nearest directory above it
that has a lockfile or `packageManager`; then a manifest only one tool reads
(`Cargo.toml`, `go.mod`, `pom.xml`, `build.gradle`, a `.csproj`). The halt
comes when a `package.json` or `pyproject.toml` has nothing to go by, or two
tools tie.

The fix is `overrides.<ws>.driver` naming the tool the workspace actually uses.
Ask the user if the evidence does not say. Do not pick from the list to make
the error go away. `"driver": "none"` plus written-out tasks is right when no
tool fits.

## Adding Lattice to a repo

1. `lattice init` (`-y` takes the proposal without asking; no TTY behaves the
   same). It proposes only what discovery would miss: the outermost projects no
   workspace file names, as globs when every sibling is one (`services/*`), and
   the tool versions the root pins (`.tool-versions`, `.nvmrc`,
   `rust-toolchain`, `.python-version`, `.ruby-version`, `.java-version`,
   `package.json` `packageManager`/`engines`, `go.mod`). The task settings in a
   `turbo.json` come over as `tasks` when its directory is one of the proposed
   workspaces or the repo root, a monorepo root included. It writes `lattice.json`, `.lattice/schema.json`, and
   `.gitignore` lines for `.lattice/cache/`, `.lattice/toolchains/`,
   `.lattice/bin/`, `.lattice/setup/` and the legacy `.lattice-setup-marker`,
   then prints every workspace it found with its driver and tasks. It refuses an existing
   `lattice.json` without `--force`, and points a 1.x one at `migrate`.
2. `lattice run build --dry-run`. Confirm each command is the one that directory
   already runs by hand.
3. Narrow `inputs` where hashing the whole workspace re-runs too much, and add
   `globalDependencies` for shared root files.
4. Keep only the `engines` whose guarantee is needed.

Existing scripts, Makefiles and CI keep working; adding Lattice stops nothing.

A subtree with its own task runner stays one workspace: list its directory, and
Lattice runs `turbo run x` or `just x` there. Put the inner runner's cache
directory in `ignore`.

## Moving from 1.x

`lattice migrate` rewrites the config by running discovery against the repo and
writing down only where it would come out differently from what 1.x said: a
workspace no workspace file names, one the 1.x config left out (`exclude`), a
name the manifest spells otherwise (`overrides.<path>.name`), an edge no
manifest states, a `scripts` command, `auto: false` (`"driver": "none"`), and a
driver chosen through a `>=0.0.0` engine. It prints what it dropped and why.
`--dry-run` prints the new file instead of writing it. Every workspace keeps the
name it had, so filters and CI keep working.

## In CI

`lattice setup` as its own step, then `lattice run`. Restore and save
`.lattice/cache` with a rolling key and set `settings.maxCacheSize`.
`--continue` reports every failure in one pass and still exits `1`. `CI` being
set already gives the plain stream. Give tasks that can hang a `timeout`.

## Verify your work

- `lattice run <tasks> --dry-run` confirms the config parses and the commands
  are right. It checks no engines.
- `lattice run <tasks> --stream -v` is a real run with every hash and miss
  reason on its own line.
- Run it again. Every task should be a hit, ending on `lattice: full power,
  nothing to run` (the live display ends on `❖❖❖ FULL POWER`).
- `lattice version` says which binary actually ran.

## References

- `references/configuration.md`: every `lattice.json` field, type, default, and
  validation error.
- `references/cli.md`: every command, flag, exit code, environment variable,
  and what each output mode prints.
- `references/toolchains.md`: discovery, driver detection, the task table per
  tool, the engine list, and provisioning.
- `references/troubleshooting.md`: symptom to cause to fix.

Full documentation: <https://latticeandcompany.github.io/lattice/docs>
