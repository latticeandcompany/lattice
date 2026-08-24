# Troubleshooting

Symptom first. Two commands answer most questions:

- `lattice run <tasks> --dry-run` validates the config and prints each
  `workspace:task` next to the exact command that would be handed to the shell.
- `lattice run <tasks> -v` runs for real and prints each task's hash and cache
  outcome as plain lines.

On a terminal without `-v` you get the live display, which prints none of the
plain lines below. `-v` adds the trace lines: the run header, each task's hash,
each cache-miss reason, each task's output, and each skipped task. The run
summary, the full-cache line, and every `lattice: warning:` line print in plain
output whether or not `-v` was passed.

## `no lattice.json found in this directory or any parent`

There is no config at or above the current directory. Either the working
directory is outside the repo, or the repo does not use Lattice yet.
`lattice init` creates one.

## `workspace 'x' has an ambiguous or undeclared driver`

Detection produced no single answer. Three distinct causes:

- two tools of the same role are present and neither is declared,
- the only candidates are runtimes, which cannot drive a named task,
- nothing recognizable is in the directory at all.

The error names the candidates it saw and prints a copy-pasteable fix. Two
correct responses, and which one is right is a fact about the repo:

- Add the suggested `engines` entry naming the tool the workspace actually uses,
  and replace the `>=0.0.0` placeholder with a real constraint.
- Set `"auto": false` and write `scripts`. This is right when there is no single
  answer to infer, such as a wrapper script or a nested repo with its own task
  runner.

Ask which tool the workspace uses. Do not pick one off the candidate list to
make the error go away. See `toolchains.md` for the full resolution rules,
including the case where a declaration loses to a higher-ranked tool.

## A declared engine is not the driver Lattice picked

Role rank is resolved before evidence. A declaration only breaks a tie among
candidates that already share the top role rank. A workspace with
`"engines": { "pnpm": ">=8" }` and a `turbo.json` resolves to `turbo`, because a
task runner outranks a package manager. Set `"auto": false` and write `scripts`
to override that.

## A task's resolved command is not what you expected

`scripts` always wins; only a task with no `scripts` entry falls back to the
driver's invoke template. Run `lattice run <task> --dry-run` and read the
command. If it is wrong, add or edit that workspace's `scripts` entry.

`--dry-run` resolves commands before any toolchain is provisioned and before
`PATH` is adjusted, so it shows the command as written, not as it would resolve
once a provisioned tool is first on `PATH`.

## `x declares scripts but no "y", so the task was skipped`

A workspace driven by `npm`, `pnpm`, `yarn`, `bun`, or `deno` can only run a task
its manifest declares as a script. A task the manifest does not declare is not
run there. The task drops out of the graph and the run carries on. The warning
fires because the manifest declares a script map *without* that task, and a typo
looks the same from the outside.

The likeliest fix is the typo the message suggests. Otherwise add the script to
the manifest, or add a `scripts` entry for the task in `lattice.json`, which
overrides inference entirely. If the workspace has nothing to do for that task,
the run is already correct. A types-only package with no `build` script has
nothing to build.

The warning is not a run failure. Do not paste a guessed command into `scripts`
to silence it. An older release fabricated `npm run <task>` in exactly this case,
and the fabricated command failed the build.

`--filter` does not narrow the warning, so a filtered run can name a workspace it
did not select. `lattice run <task> --dry-run` shows exactly which workspaces the
task resolved in.

## `x: package.json could not be parsed: ..., so every task it would have named was skipped`

The manifest is there and unreadable. No script name can be read out of it, so
nothing runs in that workspace. Fix the file. The same warning covers a manifest
that cannot be read at all and one whose `scripts` (or deno's `tasks`) is not an
object.

Lattice strips comments from a `deno.json` or a `deno.jsonc`, so a comment is not
the cause there. A `package.json` is parsed as strict JSON, the way npm parses
it, so a comment in one *is* the cause.

## `workspace 'x' has "auto": false and declares no command for task 'y'`

An `auto: false` workspace must declare a script for every task it takes part
in. Add the entry, or leave the workspace on `auto` so it can sit out tasks that
do not apply. Silent skipping happens only for `auto: true` workspaces. Under
`--filter`, the check covers the workspaces the pattern matched; one pulled in
as a dependency is asked only for the tasks its dependents need.

## `the task graph has a cycle`

Two tasks depend on each other, directly or through a chain, over same-workspace
or `^`-prefixed edges. The message names the graph, not the edge, so read the
`dependsOn` of each task involved. Nothing ran.

## `task 'dev' in workspace 'app' is persistent, so no other task may depend on it`

A persistent task must be a graph leaf. Nothing can start after a process that
never finishes. If a task needs the artifact a dev server would serve, depend on
the build step that produces it.

## ``unknown field `output` in tasks.build``

A key the config does not define at that level. The message names the container,
the position, the nearest valid field, and every field accepted there. Nothing
ran. Delete the key or correct it.

`output` for `outputs` and `input` for `inputs` are why this is fatal: either
typo silently changes what the task caches. A config carrying an extra key from
an older release has to drop it.

## A task never hits the cache

Something feeding the hash changes every run. Run with `-v`; the miss line names
the component that moved. `-v` is the only place these two lines print. The live
display carries no per-task trace:

```text
lattice: web:build: hash a1b2c3d4e5f6a7b8
lattice: web:build: cache miss: inputs changed
```

The ten component names, and what each covers:

| Name | Covers |
| --- | --- |
| `inputs` | Contents of the files `inputs` matched, minus `ignore` and minus `outputs`. With no `inputs`, the whole workspace |
| `env` | The names in the task's `env`, with their resolved values |
| `globalEnv` | The names in the root `globalEnv`, with their resolved values |
| `globalDependencies` | The root `globalDependencies` patterns and the contents of what they match |
| `manifests` | Manifests and lockfiles present in the workspace, plus lockfiles at the repo root |
| `dependencies` | The cache keys of this task's prerequisites |
| `toolchain` | The resolved toolchain identity string |
| `command` | The resolved shell command |
| `patterns` | The `inputs`, `outputs`, and `ignore` glob lists themselves |
| `environment` | Lattice version, OS, architecture, shell, workspace name, task name |

Two misses name no component:

```text
lattice: web:build: cache miss (nothing cached for this task yet)
lattice: web:build: cache miss (the entry for this key is no longer in the cache)
```

The second means the key was computed before, but its entry is gone: evicted
under `settings.maxCacheSize`, swept by `lattice prune`, or rejected as corrupt.

Common causes, by component:

- `inputs`: a glob matching a file that changes every run. A timestamp, a
  `.DS_Store`, generated output, or a tool's own state directory. Narrow
  `inputs`, or add the path to `ignore`.
- `inputs` once, right after `lattice setup`, on an older release: setup used to
  write a `.lattice-setup-marker` inside each workspace it installed into, and a
  task with no declared `inputs` hashed it. The marker now lives under
  `.lattice/setup`, which is never walked for a key, so the marker is no longer
  a cause. The next successful install deletes a stale `.lattice-setup-marker`
  left in a workspace.
- `env`: a name whose value differs across shells or machines, such as an
  absolute path or a session id. Drop it unless the command's result genuinely
  depends on it.
- `dependencies`: a prerequisite's key moved. Expected, not a problem. Look at
  the prerequisite instead.
- `manifests`: a lockfile or manifest changed. Also expected after a dependency
  bump, and it happens whether or not the lockfile is listed in `inputs`.
- `inputs` after a `chmod`: the executable bit of every hashed file is part of
  the key, because the artifact restores file modes. Nothing else about the mode
  counts, so a umask difference between machines does not move the key.
- `environment` right after an upgrade: the Lattice version is part of every
  key. One full miss per release is expected.

## A task hits the cache when it should not

The command reads a file or a variable that is not declared. `inputs` matches
only what its globs say, and only the names listed in `env` are hashed. Neither
omission warns. Widen the `inputs` glob, drop `inputs` entirely so the whole
workspace is hashed, or add the missing `env` name.

Declared `inputs` do **not** respect `.gitignore`. Only `.lattice`, `.git`,
`.hg`, `.svn`, and `.jj` are skipped. A task with no `inputs` is the opposite:
that walk honors `.gitignore` in the workspace and in every ancestor directory,
but not the user's global gitignore, and hidden files are hashed.

Symlinks are hashed either way. A symlinked file contributes the path it points
at, not the bytes on the other end, so re-pointing a link moves the key even
when both targets hold the same content. The walk descends a symlinked
*directory* when `inputs` is declared, so `inputs: ["vendor/**"]` reaches the
real files behind a symlinked `vendor`. With no `inputs`, the walk does not
descend a symlinked directory. That walk covers the whole workspace, and a
followed link would pull an unrelated tree from elsewhere on the disk into the
key.

If the file is *above* the workspace, no `inputs` glob can reach it. Patterns
are workspace-relative and `tasks` is shared across workspaces. Put the file in
the root-level `globalDependencies`, and repo-wide variables in `globalEnv`:

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"]
}
```

While investigating, `--no-cache` neither reads nor writes. Once you have found
it, `--force` re-runs and overwrites the stored entry, which is what clears a
bad one. `--no-cache` leaves the bad entry in place to be served again on the
next plain run.

## Files disappeared from an output directory after a cache hit

A hit deletes everything matching the entry's recorded `outputs` globs, then
unpacks the artifact. That is deliberate: a hit reproduces the tree the run
produced, so files the task deletes stay deleted and content-hashed filenames do
not pile up across generations. Anything you wrote by hand into a directory that
`outputs` matches is deleted on the next hit.

## `no files matched outputs [...], so nothing was cached`

```text
lattice: warning: web:build: failed to cache outputs: no files matched outputs ["dist/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
```

The task succeeded and nothing was stored, so it will re-run every time. Either
the globs are wrong, or they are written relative to the repo root instead of
the workspace. A task that legitimately produces no files, such as `test` or
`lint`, should declare no `outputs` at all.

## `outputs [...] matched only empty directories, so nothing was cached`

```text
lattice: warning: web:build: failed to cache outputs: outputs ["dist"] matched only empty directories, so nothing was cached. Check that the task writes its files where the patterns point
```

A pattern did match here, unlike the warning above, and everything it matched
was a directory holding no files. A bare `outputs: ["dist"]` covers `dist/`
itself, so an empty `dist/` counts as a match while the task has produced
nothing.

Look in the directory after a run. Either the command writes somewhere the
patterns do not cover, or it produces nothing at all. Storing that archive would
be worse than storing nothing. Every later run would hit it and restore an empty
`dist/` over whatever an uncached run had put there.

## The output looks wrong after a cache hit

A lookup is a hit only when the metadata parses, the artifact's byte length
matches what was recorded, and its sha256 matches the recorded digest. Anything
else is a miss, never a bad hit. A corrupt or half-written entry can only cost a
re-run.

Restoring can still fail after a verified hit, on permissions or disk space. That
is a warning, and the task re-runs:

```text
lattice: warning: web:build: failed to restore cached outputs: <reason>
```

## `lattice prune` reports `removed 0 artifacts` with leftover files on disk

Leftovers from an interrupted store are reclaimed only once they have sat
untouched for an hour. Until then they stay put, and their bytes count against
`settings.maxCacheSize`. Run `prune` again later, or delete the cache directory.

A store in progress and a store that died halfway through leave the same files
behind, so age is the only thing separating them. Without the wait, two
`lattice` processes sharing one checkout would delete each other's cache writes.
One process finishes its run and enforces the size limit while the other is
still writing.

## An engine version check fails on one machine and not another

That constraint is in validate-only mode: a version with no `installCmd`, so
Lattice checks whatever is on that machine's `PATH`. Either bring every
machine's host tool to the same version, or add an `installCmd` so Lattice
installs its own copy under `.lattice/toolchains/` and no host `PATH` matters.

## An engine with a version constraint is not being checked at all

Check the shape. An object with `versionCmd` but no `version` and no `installCmd`
is host mode: nothing is installed and nothing is checked, not even that the
tool exists. Add `version` to get a check, or `installCmd` to get an install.

A `version` on a name outside the well-known table, with no `versionCmd`, is now
an error in both validate-only and provisioned mode rather than a constraint
nobody checked. A `pins.json` that records `0.0.0` came from an older release.
That version was never installed, and it fed every cache key in the workspace.
Such a toolchain now records `unknown`, so the first run after upgrading misses.

## `installCmd` provisioning fails partway

Provisioning stages into a temporary directory, runs `installCmd`,
version-checks the result, then renames into the final content-addressed path. A
failure at any stage is fatal and names the stage. Nothing is left pinned, so
rerunning retries from scratch.

## `--dry-run` does not show toolchain problems

It returns before any engine is provisioned or validated. An engine failure
surfaces on a real run, or up front from `lattice setup`.

## The interactive display does not appear

Output falls back to plain lines whenever stdout is not a terminal, `CI` is set
to any value including the empty string, or `-v` was passed, or
`settings.loquacious` is `true`. On top of that, `lattice run` forces plain
output whenever the tasks it was asked for pull a `persistent` task into their
dependency closure. Redirecting into a file or a pipe is the most common
surprise.

## Color appears somewhere it should not

Color is emitted only when stdout is a real terminal, in either mode. A `-v` run
at a shell colors each `workspace:task` label; the same run piped or redirected
colors nothing. Set `NO_COLOR` to any value to suppress color everywhere without
changing the output mode.

## A run never finishes

A `persistent: true` task in the closure is meant to block. Once every other
task is done and the persistent ones are spawned, `lattice run` streams their
output and waits. To get a run that terminates, do not request the persistent
task; run its non-persistent prerequisites instead.

Four things end such a run: the first `Ctrl-C`, a `SIGTERM`, the exit of every
persistent task, or the failure of any task in the run. Lattice watches for the
signal through the whole run, so a `Ctrl-C` that arrives while the builds ahead
of the servers are still running ends the run there. Older releases needed a
second `Ctrl-C` in that case, ignored `SIGTERM` until the CI runner force-killed
the process, and went on waiting after a task failure.

A run blocks only while a persistent task is actually up. Lattice waits on every
persistent child, so one that exits stops holding the run open:

```text
web:dev: EXITED (code 1) after 1.09s
lattice: 2 tasks, 0 cached, 1 failed, 1.83s
```

Anything but a clean `0` counts as a failed task and exits non-zero.
`exited (code 0)` goes to stdout in lowercase and counts as nothing. When the
last persistent task exits, the summary prints without needing a Ctrl-C. A child
Lattice kills on the way out is never reported and never counts as a failure.

A *non-persistent* task that never finishes is a hang, not a design. Give the
tasks that can hang a `timeout`:

```text
app:slow: FAILED after 1.00s
app:slow: timed out after 1s and was stopped
```

A timeout leaves no exit code, so that line carries the duration alone.

Ctrl-C, or a `SIGTERM`, ends any run. Every running task's process group gets
`SIGTERM`, five seconds, then `SIGKILL`. The process exits `130` with:

```text
Error: interrupted. Lattice stopped the tasks that were still running
```

Nothing a task spawned survives it. Distinguish `130` from `1` if a pipeline
branches on the exit code.

## The run stopped at the first failure

That is the default. The first failing task ends the run:

```text
app:build: FAILED (code 1) after 0.31s
Error: task 'app:build' failed, stopping the run
```

`--continue` keeps independent work going instead. Its tasks that were waiting
on the failure report as skipped, and the run still exits `1`:

```text
a:build: FAILED (code 1) after 0.01s
a:test: skipped (dependency failed)
lattice: 3 tasks, 0 cached, 1 failed, 0.02s
```

A `FAILED` line carries the command's exit code and how long it ran. A task
killed by a signal, and a task stopped by its `timeout`, have no code, so the
line reads `FAILED after 30.00s`. A task that failed before its command started
has neither, so the line reads `FAILED`. The captured output prints under it, in
arrival order across both streams.

## `--filter` ran more or fewer workspaces than expected

It is a substring match on workspace `name`, never on `path`, and never a glob.
The matches are the roots of the run, so their transitive dependencies are in
the graph too, tagged `(dependency)` under `--dry-run`. Nothing that depends *on*
a match is included. A filter matching nothing prints
`lattice: no workspaces matched filter '<pattern>'.` and exits `0`.

If a prerequisite you expected is missing, check that the depending workspace
lists it in `dependsOn` and that the task's own `dependsOn` carries the `^`.

## `lattice setup` says `dependencies up to date` and the build fails on a missing dependency

The marker recording the last install is newer than every lockfile governing the
workspace. Those lockfiles are the one in the workspace and every one above it up
to the repo root, so a single root lockfile in a hoisted npm, pnpm, or yarn tree
invalidates every workspace under it.

An older release checked only the workspace's own directory. In that layout a
workspace directory holds no lockfile, so nothing could invalidate its marker.
`lattice setup` reported `dependencies up to date` forever, and only `--force`
recovered.

## `lattice setup <name>` installed nothing and exited 0

An older release selected nothing for a workspace name the config does not
declare. Current releases refuse the name: `workspace 'X' is not declared in the
\`workspaces\` array in lattice.json`, with the declared names listed. Check the
name against the `workspaces` array.

## `lattice setup` hangs, or the installer fails with no obvious reason

The installer gets no stdin, so an installer that stops for a password, a token,
or a confirmation reads end-of-file and exits non-zero. Its own output, shown as
the install runs, says what it wanted. Supply the credential without a prompt,
or run that installer once by hand outside Lattice. An older release let the
installer inherit the terminal, so the installer blocked on a prompt nothing
displayed and the command hung with no output.

## `lattice run` did nothing and exited 0

Three cases exit `0` without running anything: an empty `workspaces` array, a
filter that matched nothing, and every task in the graph hitting the cache. The
third prints:

```text
lattice: full power, nothing to run
```

That line prints only when nothing executed. It was `lattice: full cache,
nothing to run` in earlier releases, so update anything grepping for the old
string.

## `lattice stats` reports more saved time than the run took

Working as intended. The saved figure is task time, not wall clock: each hit adds
the duration the run that wrote its entry spent, whether or not those tasks would
have run at the same moment. Four cached one-minute tasks on independent branches
report `4m 00s saved` on a run that took a second.

## `lattice stats` says no runs are recorded

The ledger is a file inside the cache directory, so anything that clears the
cache clears the history: deleting `.lattice`, moving `settings.cacheDir`, or a
CI job starting from an empty cache. A run also appends nothing when it could not
store (`--no-cache`) or scheduled no task at all (a `--filter` that matched
nothing). Run a task without `--no-cache` and it fills in from there. Nothing is
recoverable and nothing needs to be: the ledger is a record, not an input.

## `.lattice/schema.json` is missing, or the editor shows a stale schema

`run`, `setup`, `prune`, and `stats` write a *missing* schema file. An existing one is
left alone on purpose, even by a newer release. Delete it and rerun any command
that loads config, or force a rewrite with `lattice init --force`. It is the one
thing under `.lattice/` meant to be committed.

## Clean slate

Everything under `.lattice/` is derived state, except `schema.json`.
`.lattice/cache/` costs only time to rebuild, `.lattice/toolchains/`
reprovisions on next need, and `.lattice/bin/` re-downloads the pinned release.
`rm -rf .lattice` is a complete, safe reset and never touches `lattice.json`.
You do not need to run `lattice init` again afterward.

Before reporting a problem, gather `lattice version`,
`lattice run <tasks> --dry-run`, and `lattice run <tasks> -v`.
