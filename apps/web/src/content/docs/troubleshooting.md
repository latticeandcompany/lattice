---
title: Troubleshooting
description: Symptoms you actually hit, with the command that shows you what went wrong.
group: Guides
order: 7
---

# Troubleshooting

Symptom, cause, fix. For the exact text of every message Lattice prints, see
[Errors](/lattice/docs/errors).

Three commands answer most of these, so reach for them before reading further:

```sh
lattice run <task> --dry-run       # every workspace, every resolved command, run nothing
lattice run <task> --stream -v     # every event as a line, with hashes and miss reasons
lattice version                    # which binary actually ran
```

## Workspaces and tasks

### `lattice.json is written for Lattice 1.x`

The config lists workspaces as objects. Run `lattice migrate`, or `lattice
migrate --dry-run` to read the result first. It keeps every workspace's name,
ordering and command. See [Upgrading to
2.0](/lattice/docs/upgrading#upgrading-to-20).

### A workspace you expected is missing

`--dry-run` lists every workspace that has the task. `lattice init` in a scratch
checkout lists every workspace Lattice sees, with its tasks. In order:

- **No file names it.** Neither a root workspace file (`package.json`
  `workspaces`, `pnpm-workspace.yaml`, Cargo `[workspace]`, `go.work`, uv,
  deno) nor `lattice.json` `workspaces` lists its directory. Add it, or a glob.
- **A glob did not reach it.** `*` stays within one directory level. Globs also
  skip hidden and gitignored directories and `node_modules`, `target`, `dist`,
  `build`, `out`, `vendor`, `venv` and `__pycache__`; name such a directory
  literally.
- **It is inside another workspace.** A Gradle, Maven or .NET build, a nested
  monorepo, a Cargo workspace in a subdirectory: its tool builds it as part of
  the outer workspace. That is deliberate. See
  [Workspaces](/lattice/docs/workspaces#one-build-is-one-workspace).
- **`exclude` matches it.**
- **It holds nothing Lattice recognizes**, so the glob match was skipped. Give it
  a command in an override.
- **It is the repo root**, which is not a workspace when it names members. Add
  `"."` to `workspaces`.

### A workspace appears that should not

A root workspace file names it, or a glob matches it. Add its path to `exclude`.

### A run halts naming a workspace Lattice cannot place

```text
error: Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

The package has no lockfile, no `packageManager` field, and neither does any
directory above it — or two tools are tied, such as a stale `package-lock.json`
beside a `pnpm-lock.yaml`. Commit the lockfile, delete the stale one, or name the
tool with `driver`. See [Driver detection](/lattice/docs/drivers).

### Lattice picked the wrong tool

A task runner beats a package manager: a `turbo.json` makes a workspace run
`turbo run <task>` even with a `pnpm-lock.yaml` beside it. Set
`overrides.<ws>.driver` to the tool you want. An `engines` entry never chooses
the tool.

### A task ran in some workspaces and not others

Each workspace runs a task only where something defines it. The live display
notes which sat out; the stream does with `-v`:

```text
· no 'lint' task in docs, web, so they sit this one out
```

For a tool with a script map (`package.json`, deno, composer, just, Taskfile,
Rakefile, `turbo.json`), add the script there. For a build tool, check the
[task table](/lattice/docs/toolchains#tasks-each-driver-can-run): some verbs
need the workspace to show the tool, such as `pytest` in a Python project's
dependencies before it gets `test`, or the Spotless plugin before a Gradle build
gets `lint`. Or write the command in an override:

```json
{ "overrides": { "web": { "tasks": { "lint": "eslint ." } } } }
```

### `web has no 'build' task. Did you mean 'biuld'?`

The workspace declares a name one typo away. Fix the name in its manifest. The
run is not failing: `web` was skipped for that task and everything else ran.

### `no workspace has a 'x' task`

No workspace defines it. A task listed only in `lattice.json`'s `tasks` map does
not count; that map configures tasks, it does not create them. The error lists
every task that does exist.

### A task's command is not the one you expected

Read it in `--dry-run`. An override's `run` wins over anything found. A Gradle
or Maven command runs through the nearest wrapper at or above the workspace
(`../gradlew build` from a subproject), or `gradle` and `mvn` when there is
none. `--dry-run` shows the command as written, before installed tools and
`node_modules/.bin` are on `PATH`.

### A run from a subdirectory ran fewer workspaces

A run inside a workspace covers that workspace and what it depends on, and says
`in <ws>, the workspace you are in`. Run from the root, or pass `--filter`.

### Arguments after `--` did not reach the tool

They go to the tasks named on the command line, never to prerequisites. They
pass through as written, so a tool with its own separator still needs it:
`lattice run test -- -- --nocapture` for Cargo's test harness. `npm run` is the
one command that gets its `--` added for you.

### `command not found` for a tool the project installed

Lattice adds the project's dependency directories (`node_modules/.bin`,
`.venv/bin`, `vendor/bin`) to a task's `PATH`, but only the ones that exist. On a
fresh clone none do. Run `lattice setup`.

### A tool that prompts fails at once

Every task runs with an empty stdin and no controlling terminal. `sudo`, `ssh`,
`gpg` or an installer that asks a question gets end-of-file and exits with its
own message, instead of waiting on a prompt nothing shows. A task that read the
terminal used to be stopped by the shell and hang the run forever, which is why
this changed. Provide credentials without a prompt: an agent, a token in the
environment.

### `unknown field` on a key you believe is valid

The message names where the key sits and the fields accepted there. Keys from
1.x (`auto`, `scripts`, a workspace object's `path`) are gone; see [Upgrading to
2.0](/lattice/docs/upgrading#upgrading-to-20).

### `` `overrides` key 'x' names no workspace ``

A key with no `/` is a workspace name, and names come from manifests:
`packages/ui` may be named `@acme/ui`. Use the name the error lists, or key by
path: `"packages/ui"`, or `"./tools"` for a top-level directory.

### `the task graph has a cycle through <ws>:<task>`

Workspace edges come from manifests, so two packages that list each other as
dependencies form a cycle. Check both manifests and every `dependsOn` involved.

### `task 'start' in workspace 'x' is persistent, so no other task may depend on it`

`dev`, `start`, `serve` and `watch` are persistent by default. A `start` that is
a one-shot command needs `"start": { "persistent": false }`.

## Caching

### A task misses the cache every run

Something in its key changes every run. `--stream -v` names it:

```text
lattice: web:build: cache miss: inputs changed
```

- `inputs`: a file that changes every run, such as a timestamp, a `.DS_Store`,
  or a tool's state directory. Narrow `inputs`, or add the path to `ignore`.
- `env`: a value that differs between shells, such as an absolute path.
- `dependencies`: a prerequisite's key moved. Look at the prerequisite.
- `manifests`: a manifest or lockfile changed. Expected after a dependency bump.
- `command`: the command changed, including arguments after `--`.

A task that stores nothing also misses every run; see the outputs warning below.

### A task hits the cache when the result is stale

The command reads something undeclared. `inputs` hashes only what its globs
match, and `env` only the names it lists. Widen `inputs`, remove it to hash the
whole workspace, or add the variable to `env`. A file above the workspace goes in
`globalDependencies`. `--force` replaces the bad entry.

### Files disappeared after a cache hit

A hit clears what the task's `outputs` match, then restores the stored copy. A
`build` has default outputs: `dist/**` for JavaScript and Python packages,
`build/**` beside each Gradle build file, `target/**` beside each `pom.xml`, `**/bin/Debug/**` and
`**/bin/Release/**` for .NET. Keep files you
write by hand out of those, or set `outputs` for the task.

### A task warns about its outputs and is never cached

```text
lattice: warning: web:build: failed to cache outputs: no files matched outputs ["dist/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
```

The task passed and wrote nothing the patterns cover. With a default output,
the build writes somewhere else (`.next/`, `out/`, `build/`): set `outputs` for
it. `matched only empty directories` means a pattern matched an empty directory.

### Every task misses after an upgrade

The Lattice version is part of every key. One full miss per version.

### `lattice prune` leaves files behind

Leftovers from an interrupted run are reclaimed once they are an hour old, so
two `lattice` processes can share a cache without deleting each other's writes.

### The saved figure is larger than the run took

It is task time, not wall clock: four cached one-minute tasks save four minutes
however long the run took.

## Toolchains

### `workspace 'x' cannot run` and an engine error

An engine failed its check or install before anything started. The lines under
it say what was wanted, found and run, and what to do. See the table in
[Errors](/lattice/docs/errors#toolchain-resolution-and-provisioning).

### An engine check fails on one machine and passes on another

A constraint with no `installCmd` checks each machine's own `PATH`. Bring every
machine to the same version, or add an `installCmd` so Lattice installs its own
copy under `.lattice/toolchains`.

### A `gradle` engine fails with `gradle` installed, or passes without it

A `gradle` or `maven` engine is checked through the build's wrapper
(`./gradlew --version`), because the wrapper decides the version the build
uses. The host `gradle` is checked only when there is no wrapper at or above the
workspace.

### The pinned tool is not the one in your terminal

A pin applies to the tasks Lattice runs. `eval "$(lattice env)"` puts the same
`PATH` in your shell.

### `--dry-run` says nothing about toolchains

It checks no engines. A real run, `lattice setup`, or `lattice env` does.

### `lattice setup` says `dependencies up to date` but a dependency is missing

No lockfile in the workspace or above it is newer than its install marker. Run
`lattice setup --force`.

## Output

### You expected the live display and got plain lines

The stream is used when stdout is not a terminal, when `CI` is set to anything,
with `--stream`, and for any run that includes a persistent task (`dev`,
`start`, `serve`, `watch`).

### You passed `-v` and still got the live display

`-v` adds detail to either display; it no longer switches. Pass `--stream`.

### A run with a dev server in it never finishes

That is what a persistent task does. `Ctrl-C` stops it. The run also ends when
every server exits or any task fails.

### A `FAILED` line has no exit code

A task killed by a signal, or stopped for overrunning its `timeout`, has none;
a timeout also prints `timed out after <limit> and was stopped` as a warning
just before the line. A task that never started prints only `FAILED`, with the
reason under it.

### The run warns that a process was left holding its output

A task started something in a process group of its own that still holds the
output open. Lattice waits half a second, warns, and moves on; that process
keeps running. Stop it by port or name.

## Running

### `--filter` ran more or fewer workspaces than expected

It is a substring match on the workspace name, never the path. What the matches
depend on comes along, tagged `(dependency)`; what depends on them does not.
Names come from manifests, so `--filter web` also matches `@acme/web-ui`.

### `lattice run` did nothing and exited 0

No workspaces, a filter that matched nothing, or every task a cache hit (`FULL
POWER`).

### `.lattice/schema.json` looks stale in your editor

Any command that loads the config rewrites it when it differs from the running
binary's copy. Run one, and commit the file.

## Start over

Everything under `.lattice/` is derived, apart from the schema copy:

| Path | What deleting it costs |
| --- | --- |
| `.lattice/cache/` | Cached results. Every task reruns once |
| `.lattice/toolchains/` | Installed engines, reinstalled from each `installCmd` when next needed |
| `.lattice/bin/` | Lattice versions the repo pinned, re-downloaded when next needed |
| `.lattice/schema.json` | Rewritten by the next command |

`rm -rf .lattice` is a complete reset that never touches `lattice.json`.

## Gather information for a bug report

| Command | What it gives you |
| --- | --- |
| `lattice version` | The binary that actually ran |
| `lattice run <tasks> --dry-run` | Every workspace and resolved command |
| `lattice run <tasks> --stream -v` | Every hash, cache decision, and each task's output |
