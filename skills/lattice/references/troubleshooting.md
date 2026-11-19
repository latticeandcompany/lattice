# Troubleshooting

Symptom first. Two commands answer most questions:

- `lattice run <tasks> --dry-run` loads the config, discovers the workspaces,
  and prints each `workspace:task` next to the exact command it would run.
- `lattice run <tasks> --stream -v` runs for real and prints every event as a
  plain line, with each task's hash and cache-miss reason.

Errors print as `error: <message>` with causes indented under it. Warnings
print in every mode; notes print on the live display, and in the stream only
with `-v`.

## `no lattice.json found in this directory or any parent`

The working directory is outside the repo, or the repo does not use Lattice yet.
`lattice init` creates a config.

## `lattice.json is written for Lattice 1.x ...`

The config lists workspaces as objects. Run `lattice migrate` (add `--dry-run`
to read the result first). It keeps every workspace name, ordering and command.
Do not translate it by hand: the point of the new format is what it leaves out,
and `migrate` checks the result against the repo before writing it.

After migrating, a workspace named after a scoped package keeps its old short
name through `overrides.<path>.name`. A workspace added later takes its
manifest's full name, such as `@acme/ui`.

## A workspace is missing

Run `lattice run build --dry-run` and compare against what you expected. In
order:

- It is not named by a root workspace file (`package.json` `workspaces`,
  `pnpm-workspace.yaml`, Cargo `[workspace]`, `go.work`, uv, deno) and not by
  `lattice.json` `workspaces`. Add its directory, or a glob, to `workspaces`.
- A glob skips hidden and gitignored directories, and does not walk through
  `node_modules`, `target`, `dist`, `build`, `out`, `vendor`, `venv` or
  `__pycache__` (`packages/*` still matches `packages/build`; `packages/**`
  does not). Name such a directory literally.
- `*` does not cross a `/`. `apps/*` does not reach `apps/web/admin`; use
  `apps/**` or list it.
- It is inside another workspace that Lattice runs as one: a Gradle, Maven or
  .NET build, a nested monorepo, a Cargo workspace in a subdirectory. Its tool
  builds it as part of the outer workspace; that is deliberate.
- It is matched by `exclude`.
- The directory holds nothing Lattice recognizes, so a glob match was skipped.
  Give it a command: `"overrides": { "./tools": { "tasks": { "build": "make" } } }`.
- It is the repo root. The root is not a workspace when it names members; add
  `"."` to `workspaces`.

## A workspace appears that should not

A root workspace file names it, or a glob matches it. Add its path to `exclude`.
Gradle subprojects never appear as separate workspaces unless a glob names
them; list the build's root instead (`"jvm"`, not `"jvm/*"`), because two
builds of one Gradle project at once fight over its locks.

## `Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'x'`

A `package.json` (or `pyproject.toml`) with no lockfile, no `packageManager`, no
`[tool.*]` table, and none of those in any directory above it, or two tools
tied on equal evidence. Add the fix it prints, naming the tool the workspace
actually uses:

```json
"overrides": { "x": { "driver": "pnpm" } }
```

Ask which tool the workspace uses if its files do not say. Do not pick one off
the list to make the error go away. Committing the lockfile also resolves it.

## The driver Lattice picked is not the one you use

Role rank decides first: a task runner (`turbo.json`, a `justfile`, a
`Rakefile`) beats a package manager, which beats a build tool. A workspace with
`pnpm-lock.yaml` and a `turbo.json` runs `turbo run <task>`. Set
`overrides.<ws>.driver` to the tool you want; it bypasses detection entirely.
An `engines` entry never selects a driver.

## A task did not run in a workspace

Every workspace without the task sits the run out. The live display notes it
(`no 'lint' task in docs, web, so they sit this one out`); the stream does with
`-v`. Check what the workspace can run: `--dry-run` for the task, or the table
in `toolchains.md`.

- A `package.json`, deno, composer, just, Taskfile, Rakefile, pdm or Pipfile
  workspace runs only what its file declares. Add the script, or write the
  command in an override.
- A verb that needs a dependency (`pytest`, `ruff`, `black`, `flake8`, `mypy`,
  `rspec`, `rubocop`, `:credo`) or a plugin (Spotless, Android, `application`,
  Spring Boot) is offered only when the manifest shows it.
- A Gradle task nobody registered in the build script is invisible to Lattice.
  Write it out: `"tasks": { "jib": "./gradlew jib" }`.
- An override task object with no `run` only changes a task the workspace
  already has.

## `web has no 'build' task. Did you mean 'biuld'?`

The workspace declares a name one or two edits from the task, which is what a
typo looks like. Fix the name in the manifest. The warning is not a failure: the
task was skipped there and the run carried on.

## `no workspace has a 'x' task`

No workspace defines the task, so the run is refused, with the nearest name and
every task that exists. A task listed only under `lattice.json` `tasks` does not
count; that map configures tasks, it does not create them.

## `web: package.json could not be parsed: ..., so it runs none of its scripts`

The manifest is there and unreadable, so no script can be read out of it. Fix
the file. `deno.json`, `deno.jsonc` and `turbo.json` may carry comments;
`package.json` may not.

## A task's command is not what you expected

`--dry-run` shows it. An override's `run` wins over anything discovered. For
Gradle and Maven the command goes through the nearest wrapper at or above the
workspace (`../gradlew build` from a subproject), or `gradle`/`mvn` with none.
`--dry-run` shows commands as written, before toolchains or dependency bin
directories are on `PATH`.

## Arguments after `--` went somewhere unexpected

They go to the tasks named on the command line, in the workspaces the run is
for, never to prerequisites. They are passed as written: `lattice run test --
--nocapture` gives Cargo `cargo test --nocapture`, which Cargo rejects; its
harness needs `lattice run test -- -- --nocapture`. `npm run` and a Taskfile
task are the two that get a `--` inserted for you.

## A run from a subdirectory ran fewer workspaces

Running inside a workspace scopes the run to it and what it depends on, and the
header or first line says `in <ws>, the workspace you are in`. Run from the
root, or pass `--filter`, for more.

## `<ws>, the workspace you are in, has no 'x' task`

The run started inside a workspace that has none of the tasks named, which
would otherwise run nothing and exit `0`. Run it from the repo root, or pick
workspaces with `--filter`.

## `sh: <tool>: command not found`

- The dependency is not installed yet. Lattice adds the project's dependency bin
  directories (`node_modules/.bin`, `.venv/bin`, …) only when they exist. Run
  `lattice setup`.
- The tool is neither a project dependency nor on the host `PATH`. Add it as a
  dependency, or give its engine an `installCmd`.
- For an engine, the check says so first: `engine 'go' is not installed, or not
  on PATH`.

## A tool that prompts fails at once

Every task runs with an empty stdin and no controlling terminal, so `sudo`,
`ssh`, `gpg`, or an installer asking a question gets EOF or an error and exits.
That is deliberate: a task that read the terminal used to be stopped by the
shell and hang the run forever, which is how `./gradlew` hung. Supply
credentials without a prompt (an agent, a token in the environment).

## `error: workspace 'x' cannot run` / `engine '<name>' ...`

An engine failed its check or install before any task started. The indented
lines say what was wanted, found, run and printed, and what to do. The common
ones:

- `does not match the version this repo pins`: the host tool's version. Install
  the pinned version, or add an `installCmd` so no host `PATH` matters. A check
  that passes on one machine and fails on another is validate mode doing its
  job.
- `is not installed, or not on PATH`: install it, or add an `installCmd`.
- `could not be installed: its installCmd exited with code <n>`: the output
  follows. The command runs with `$LATTICE_TOOLCHAIN_DIR` set to an empty
  directory it should install into.
- `has a version constraint but no way to check the installed version`: a name
  outside the well-known table needs `versionCmd`.
- A `gradle` engine failing on a machine with no `gradle`: it is checked through
  the build's wrapper; if the workspace has no `gradlew` above it, the host
  `gradle` is what gets checked.

## ``unknown field `output` in tasks.build``

A key the config does not define. The message names the container, position,
nearest field and every accepted field. `output` and `input` are why this is
fatal: either typo would change what a task caches without a word.

## `` `overrides` key 'x' names no workspace ``

The key is read as a name (no glob character, and no `/` unless it starts with
`@`) and no workspace has it.
Workspace names come from manifests, so `apps/web` may be named `@acme/web`;
`--dry-run` or `lattice init`'s table shows the names. Match by path instead:
`"apps/web"`, or `"./tools"` for a top-level directory.

## `workspace 'x' depends on 'y', which is not a workspace`

An override's `dependsOn` names a workspace that does not exist under that
name. Use the discovered name.

## `the task graph has a cycle through <ws>:<task>`

Two tasks depend on each other through `dependsOn`. Workspace edges come from
manifests too, so a dev-dependency cycle between two packages is a cycle here.
Read the `dependsOn` of each task involved and each manifest's dependencies.

## `task 'dev' in workspace 'app' is persistent, so no other task may depend on it`

A persistent task must be a leaf. `dev`, `start`, `serve` and `watch` are
persistent by default; a `start` that is a one-shot command needs
`"start": { "persistent": false }`.

## A task never hits the cache

Something feeding the key changes every run. `--stream -v` names it:

```text
lattice: web:build: hash a1b2c3d4e5f6a7b8
lattice: web:build: cache miss: inputs changed
```

| Component | Common cause |
| --- | --- |
| `inputs` | A file that changes every run: a timestamp, `.DS_Store`, a tool's state directory. Narrow `inputs` or add to `ignore` |
| `env` | A value that differs per shell, such as an absolute path |
| `dependencies` | A prerequisite's key moved. Expected; look at the prerequisite |
| `upstream` | A file changed in a workspace this one depends on. Expected |
| `manifests` | A manifest or lockfile changed. Expected after a dependency bump |
| `command` | The command changed, including arguments after `--` |
| `environment` | A Lattice upgrade. One full miss per release |

`cache miss (nothing cached for this task yet)` and `cache miss (the entry for
this key is no longer in the cache)` name no component; the second means the
entry was evicted or rejected as corrupt.

## A task hits the cache when it should not

The command reads something undeclared. `inputs` hashes only what its globs
match, `env` only the names it lists. Widen `inputs`, drop it so the whole
workspace is hashed, or add the `env` name. A file above the workspace goes in
`globalDependencies`. Declared `inputs` ignore `.gitignore`; the whole-workspace
walk honors it. `--force` replaces a bad entry; `--no-cache` leaves it in place.

## Files disappeared after a cache hit

A hit deletes what the entry's `outputs` match, then unpacks the archive. That
includes a default output: an npm, pnpm, yarn, bun, deno, uv, poetry or pdm
workspace's `build` owns `dist/**`; Gradle's and Maven's own the `build/` or
`target/` beside each build file; .NET's own every `bin/Debug` and
`bin/Release` in the build. Keep hand-written files out of those, or set
`outputs` for the task.

## `no files matched outputs [...], so nothing was cached`

```text
lattice: warning: web:build: failed to cache outputs: no files matched outputs ["dist/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
```

The task passed and stored nothing, so it re-runs every time. With a default
output this means the build writes somewhere else (`.next/`, `out/`); set
`outputs` for it. `outputs [...] matched only empty directories` is the same for
a pattern that matched an empty directory.

## The live display does not appear

The stream is used when stdout or stderr is not a terminal, when `CI` is set to
anything, with `--stream`, and for any run that includes a persistent task. `-v`
no longer switches modes.

## A run never finishes

A persistent task is meant to block; `dev` is persistent by default. Run its
non-persistent prerequisites instead, or stop it with `Ctrl-C`. The run also
ends when every persistent task exits or any task fails. A persistent task that
exits prints `EXITED (code 1) after 1.09s` and fails the run.

A non-persistent task that never finishes is a hang. Give it a `timeout`:

```text
app:slow: running
app:slow: starting
lattice: warning: app:slow: timed out after 1s and was stopped
app:slow: FAILED after 1.00s
```

The timeout is a warning, so it prints whether or not the task had already
streamed output; on the live display it is a `! app:slow: timed out after 1s
and was stopped` line above the task's `✗`. A timeout leaves no exit code, so
the `FAILED` line carries the duration alone.

`Ctrl-C` or `SIGTERM` stops every task's process group (`SIGTERM`, five
seconds, `SIGKILL`) and exits `130` after the summary, with no error line. A
process a task started in a group of its own is out of reach; if it holds the
output open Lattice warns `a task left a process holding its output open; it is
still running` and exits. A second
`Ctrl-C` exits at once.

## The run stopped at the first failure

The default. `--continue` keeps independent work going, reports every failure,
and still exits `1`; tasks waiting on a failure print `skipped (dependency
failed)` under `-v`.

## `--filter` ran more or fewer workspaces than expected

It is a substring match on workspace name, never on path. Matches are the roots
of the run; what they depend on comes along, tagged `(dependency)`. Workspace
names now come from manifests, so `--filter web` also matches `@acme/web-ui`.

## `lattice setup` says `dependencies up to date` and the build fails

The workspace's marker under `.lattice/setup` is newer than every lockfile its
driver writes, in the workspace and above it. Gradle and Maven have no lockfile
Lattice reads, so their install never re-runs on its own. Run
`lattice setup --force`.

## `lattice run` did nothing and exited 0

A repo with no workspaces (`lattice: found no workspaces. ...`), a filter that
matched nothing, or every task a cache hit (`lattice: full power, nothing to
run`, or `❖❖❖ FULL POWER` on the live display).

## `lattice stats` reports more saved time than the run took

The saved figure is task time, not wall clock.

## The editor validates against an old schema

Every command that loads the config rewrites `.lattice/schema.json` when it
differs from the binary's copy. Run any `lattice` command. Commit the file.

## Clean slate

Everything under `.lattice/` except `schema.json` is derived. `rm -rf .lattice`
is a complete reset that never touches `lattice.json`.

Before reporting a problem, gather `lattice version`, `lattice run <tasks>
--dry-run`, and `lattice run <tasks> --stream -v`.
