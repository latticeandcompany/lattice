---
title: Errors
description: Every error message Lattice emits, what raises it, and how to resolve it.
group: Reference
order: 5
---

# Errors

Every message Lattice fails on, plus the warnings it prints and the two messages
that read like failures and are not. Entries are grouped by the stage of a run
that raises them. Each message appears on one line, the way the program prints
it, so a find in this page (Ctrl-F, or Cmd-F) matches what you just saw in your
terminal.

A fatal error goes to stderr as `Error: ` followed by the message, and the
process exits `1`. When one error wraps another, a `Caused by:` block follows
with the inner message. A warning does not stop the run. Raw output prefixes a
warning with `lattice: warning: `, and adds `<workspace>:<task>: ` after that
when the warning belongs to one task. The interactive display prefixes it with a
yellow `warn` instead. Each entry below says which it is.

For symptom-first fixes, see [Troubleshooting](/lattice/docs/troubleshooting).
For the fields these messages name, see
[Configuration](/lattice/docs/configuration). For the exit codes, see the [CLI
reference](/lattice/docs/cli#exit-codes).

## Config loading and validation

Raised while finding, reading, parsing, and validating `lattice.json`. Nothing
has run at this point.

### No `lattice.json` found

```text
no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

Raised by `lattice run`, `lattice setup`, `lattice prune`, and `lattice upgrade`
when the walk up from the working directory reaches the filesystem root without
finding a `lattice.json`. All four print the same text. Fatal. Run `lattice
init`, or change into the repo.

### Config file unreadable or unparseable

```text
failed to read lattice.json from /repo
```

```text
failed to parse lattice.json
```

The first wraps a filesystem error: a permission denial, or a directory named
`lattice.json`. The second is the head of a parse failure, and the `Caused by:`
block underneath carries the reason with its line and column. Both fatal.

### Unknown key in `lattice.json`

```text
unknown field `output` in tasks.build (lattice.json line 3, column 32)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

Raised while parsing, for a key the config does not accept at that level. The
first line names the key, the object holding it, and where it sits in the file.
The container reads the way it does in the file: `workspaces[1]`, `tasks.build`,
`engines.node`, or `at the top level of lattice.json`. `Did you mean` appears
when an accepted field is within one or two edits. `Fields accepted here` lists
the accepted keys for that container, so the list differs by container. Fatal,
and raised before any workspace is read. Delete the key, or correct it to the
field the message names. See
[Configuration](/lattice/docs/configuration#unknown-keys).

### Engine value in neither accepted form

```text
invalid type: integer `20`, expected a version constraint string or an engine object
```

An `engines` entry whose value is neither a version-constraint string such as
`">=20.0.0"` nor an object such as `{ "version": ">=20.0.0" }`. Fatal, under a
`Caused by:` beneath `failed to parse lattice.json`.

### Engine declared in the string form Lattice cannot check

```text
engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
```

Raised during validation for an `engines` entry written as a bare version string
whose name is not on the well-known engine list. `in root` becomes `in workspace
'<name>'` for a per-workspace engine. Fatal. Switch to the object form the
message shows. The well-known list is on
[Toolchains](/lattice/docs/toolchains).

### Workspace has an empty path

```text
workspace 'web' has an empty path
```

The `path` field is present but blank or whitespace-only. Fatal.

### Workspace path is not relative to the repo root

```text
workspace 'abs' has a path '/tmp/x' that is not relative to the repo root. Write every workspace path relative to the repo root
```

The `path` is absolute. Fatal. Write it relative to the directory holding
`lattice.json`.

### Workspace path points outside the repo

```text
workspace 'esc' has a path '../outside' that points outside the repo root. Every workspace path must stay inside the repo
```

The `path` climbs above the repo root with `..`. Fatal. The workspace directory
bounds which files are hashed, which files the `outputs` globs match, and which
files a cache hit clears before it unpacks, so it has to sit inside the repo.

### Workspace `dependsOn` names an undeclared workspace

```text
workspace 'api' depends on 'cor', which is not a declared workspace
Did you mean `core`?
Declared workspaces: core, api
```

A name in a workspace's `dependsOn` matches no entry in `workspaces`. Fatal. An
unresolvable name builds no edge, so `^task` would expand to nothing and the
ordering the config was written to guarantee would not happen. `Did you mean`
appears when a declared name is close enough to be a typo. A workspace that
lists itself gets its own message:

```text
workspace 'api' lists itself in `dependsOn`
```

Also fatal.

### Task `dependsOn` names an undefined task

```text
task 'build' depends on 'codegen', but 'codegen' is not defined in `tasks`
Did you mean `codegin`?
Defined tasks: build, test, codegin
```

A name in a task's `dependsOn` has no entry in the `tasks` map. Fatal, for the
same reason: the prerequisite would resolve to no node and not run. The `^`
prefix is stripped before the check, so `^build` is checked against `build`. A
task that lists itself gets its own message:

```text
task 'build' lists itself in `dependsOn`
```

Also fatal.

### Duplicate workspace name

```text
duplicate workspace name 'web'. Every workspace name must be unique
```

Two entries in the `workspaces` array share a `name`. Fatal. A second check for
the same condition exists at the discovery stage with different text, but this
one runs first, so this is the message you see. See [Duplicate workspace name or
path at discovery](#duplicate-workspace-name-or-path-at-discovery).

### Invalid cache size

Raised while parsing a cache size, which happens for `settings.maxCacheSize` in
`lattice.json` and for `lattice prune --max-size`:

```text
cache size is empty
```

```text
cache size 'XB' does not start with a number
```

```text
could not read the number in cache size '10..5GB'
```

```text
unknown cache size unit 'XB' in '10XB'. Use B, KB, MB, GB, or TB
```

All fatal. A valid size is a number followed by `B`, `KB`, `MB`, `GB`, or `TB`,
case-insensitive and base 1024, or a bare integer of bytes: `"10GB"` or
`"1048576"`. The third message carries the number-parsing detail under `Caused
by:`, such as `invalid float literal`. From `settings.maxCacheSize` all four
arrive under `failed to parse lattice.json`. From `--max-size` they arrive on
their own.

### Invalid duration

Raised while parsing a task's `timeout`:

```text
duration is empty
```

```text
duration 'XB' does not start with a number
```

```text
could not read the number in duration '10..5s'
```

```text
unknown duration unit 'y' in '10y'. Use ms, s, m, or h
```

```text
duration '0s' must be greater than zero
```

All fatal, and all arrive under `failed to parse lattice.json`. A valid duration
is a number followed by `ms`, `s`, `m`, or `h`, or a bare number of seconds:
`"10m"`, `"90s"`, `"90"`. A duration that rounds below one second is rejected
rather than rounded to zero, which would kill the task the moment it started.

### `prune` has no limit to work to

```text
no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json
```

`lattice prune` ran with neither `--max-size` nor `settings.maxCacheSize`. Fatal.
Pass the flag, or set the field.

## Workspace discovery and driver detection

Raised while resolving the `workspaces` array: checking that each path is a
directory, and picking each workspace's driver from the evidence in it. See
[Driver detection](/lattice/docs/drivers).

### Ambiguous or undeclared driver

The evidence ladder's halt. Fatal. The text is built from what Lattice found in
the workspace directory, so it takes three shapes.

No evidence at all, such as a directory holding only a `.nvmrc`:

```text
workspace 'app' has an ambiguous or undeclared driver.
Lattice detected no driver. The directory holds no lockfile, no wrapper, and no native declaration.
Declare the driver in lattice.json, under this workspace:
  "auto": false, "scripts": { "build": "<command>" }
```

A bare ecosystem marker with no tool-unique signal, such as a `package.json`
with no lockfile:

```text
workspace 'app' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

Two drivers of the same role at once, such as both `bun.lockb` and
`pnpm-lock.yaml`:

```text
workspace 'web' has an ambiguous or undeclared driver.
Candidate drivers: bun, pnpm
Declare the driver in lattice.json, under this workspace:
  "engines": { "bun": ">=0.0.0" }
```

Paste the suggested line into that workspace's entry in `lattice.json`, naming
whichever driver you use if it is not the one suggested. The suggestion always
resolves the halt: an `engines` entry when a candidate can drive tasks, and the
`scripts` form when none can, because a runtime alone never drives a named task.
This halt fires only for a workspace whose `auto` is `true`, which is the
default.

### Workspace path is not a directory

```text
workspace path 'apps/gone' does not point to a directory. A workspace path is one literal directory, not a glob
```

A configured `path` does not resolve to an existing directory. Fatal. See
[Workspaces](/lattice/docs/workspaces).

### Duplicate workspace name or path at discovery

```text
duplicate workspace name 'web' in lattice.json
```

```text
duplicate workspace path 'apps/web' in lattice.json
```

Raised during discovery. Both fatal. Validation rejects a duplicate `name`
earlier with different text, so in practice the path form is the one you see. It
fires even when two entries carry different `name`s, once their paths
canonicalize to the same directory.

## Task graph construction

Raised while expanding the root `tasks` map across the resolved workspaces into
the execution graph. See [Task graph](/lattice/docs/task-graph).

### Unknown task

`lattice run` checks every task name you passed before it builds any graph:

```text
task 'lint' is not defined in the `tasks` map in lattice.json. Defined tasks: build, test
```

The tail becomes `lattice.json defines no tasks` when the `tasks` map is empty.
Fatal. Graph construction carries the same check for callers that skip the
pre-check, without the tail:

```text
task 'lint' is not defined in the `tasks` map in lattice.json
```

Through the CLI you always see the first form.

### A manual workspace declares no command for a requested task

```text
workspace 'legacy' has "auto": false and declares no command for task 'build'. Add the command under this workspace's "scripts" map in lattice.json
```

A workspace with `auto: false` infers nothing and never invents a command, so a
requested task with no entry in that workspace's `scripts` map halts the run.
Fatal. An `auto: true` workspace instead skips a task its driver does not
provide. A `--filter`ed run holds only the matched workspaces to this check: one
pulled in as a dependency is asked for the tasks its dependents need, not for
the task you named.

### A persistent task is depended on

```text
task 'dev' in workspace 'web' is persistent, so no other task may depend on it
```

A `persistent: true` task is not expected to exit, so nothing that waits for it
could ever start. Fatal. See [Persistent
tasks](/lattice/docs/persistent-tasks).

### The task graph has a cycle

```text
the task graph has a cycle
```

The `dependsOn` edges, same-workspace or `^`-prefixed, form a cycle, so no
topological order exists. Fatal. The message does not name the tasks in the
cycle.

## Toolchain resolution and provisioning

Raised while satisfying each `engines` constraint. The shape of the constraint
picks host mode, validate-only, or provisioning. Host mode installs nothing and
checks nothing, so it raises none of these. The other two each fail their own
way. See [Engines and provisioning](/lattice/docs/engines).

### Validate-only failures

For an engine with a version constraint and no `installCmd`.

```text
engine 'alpes': version command `alpes --version` failed:
sh: alpes: command not found
```

The version command exited non-zero. The combined stdout and stderr follow on
the next lines. Most often the tool is not installed.

```text
engine 'alpes': could not read a version from the output of `echo unknown`:
unknown
```

The version command succeeded, but its output held no recognizable number.
Lattice takes the first run of digits and dots it finds. The output follows on
the next lines.

```text
engine 'alpes' has a version constraint but no way to check the installed version. 'alpes' is not a well-known engine, so add a `versionCmd` to it
```

An object-form engine carries a `version` but no `versionCmd`, and its name has
no built-in version command. Add a `versionCmd`.

```text
engine 'alpes' on PATH is 1.4.0, which does not satisfy the constraint '>=2.0.0'
```

The tool is present and its version parsed, but the version does not satisfy the
constraint. Install a satisfying version on `PATH`, loosen the constraint, or
add an `installCmd` so Lattice provisions the version itself.

All four fatal, and all four raised before any task starts.

### Provisioning failures

For an engine with an `installCmd`.

```text
failed to create toolchain dir /repo/.lattice/toolchains/alpes
```

Wraps a filesystem error creating the content-addressed install directory.

```text
engine 'alpes': installCmd failed:
curl: (6) Could not resolve host: alpes.example
```

The `installCmd` exited non-zero. Its combined stdout and stderr follow.

```text
failed to move toolchain into place: /repo/.lattice/toolchains/alpes/tmp-a1b2c3d4 -> /repo/.lattice/toolchains/alpes/1.4.0-a1b2c3d4
```

The install succeeded, but renaming the staged directory to its final,
version-stamped path failed. Usually a filesystem or permission problem.

```text
engine 'alpes': version command `alpes --version` failed after install:
sh: alpes: command not found
```

The install command exited `0`, but the version command run against the freshly
installed `bin` directory failed. Most often the engine's `bin` path is wrong,
so the installed tool is not on the `PATH` the check uses.

```text
engine 'alpes' provisioned 1.4.0, which does not satisfy the constraint '>=2.0.0'
```

The freshly installed tool reports a version the declared constraint rejects.
The `installCmd` installed the wrong version.

All five fatal. One more failure underlies both modes:

```text
failed to spawn `alpes --version`
```

Wraps an OS error spawning the shell that runs a version or install command. In
practice, `sh` itself is missing.

## Cache operations

Raised by cache-key computation and cache storage. See
[Caching](/lattice/docs/caching) for the model and [Cache
internals](/lattice/docs/cache-internals) for the on-disk format.

A lookup is a hit only when the metadata parses, the artifact's recorded size
matches the file on disk, and the artifact's sha256 matches the recorded digest.
Everything else is a miss, and the task runs. A lookup never raises an error and
never warns: unparseable metadata, a missing artifact, a truncated artifact, and
a digest mismatch are all misses.

### A cache key could not be computed

```text
failed to compute cache key: failed to read input file /repo/apps/app/src/main.rs
```

Raised before a task starts, when reading a file the key covers fails. Usually a
file that disappeared between the glob and the read, or one the process cannot
read. Fatal for that task, and the run then behaves as it does for any task
failure. This is the only cache error that fails a task.

The wrapped part names the file. Every file the key covers uses the same
wording, whether it is an `inputs` match, a manifest, a lockfile, or a
`globalDependencies` match:

```text
failed to read input file <path>
```

```text
failed to stat <path>
```

```text
failed to read <path>
```

```text
<path> changed while Lattice was hashing it. Expected 4096 bytes, read 2048
```

The last one fires when a file is rewritten mid-hash, which would otherwise
record a key for bytes that never existed together.

### Restore and store warnings

Warnings, carrying the workspace and task they belong to, so raw output prints
each of these after `lattice: warning: app:build: `. A restore failure falls
through to running the task. A store failure is reported after a successful run.

```text
failed to restore cached outputs: failed to unpack artifact into /repo/apps/app
```

```text
failed to cache outputs: no files matched outputs ["dist/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
```

```text
failed to cache outputs: failed to create cache dir /repo/.lattice/cache
```

The second is the one you are most likely to hit: the task succeeded and
declared `outputs`, and none of the patterns matched a file. Nothing is stored,
so the next run misses again.

Each wraps one of the cache-storage errors, which never appear on their own:
`failed to create cache dir <path>`, `failed to create artifact <path>`, `failed
to add <path> to artifact`, `failed to digest artifact <path>`, `failed to write
artifact <path>`, `failed to write cache metadata at <path>`, `failed to open
artifact <path>`, and `failed to unpack artifact into <path>`. None of them
fails the run.

### `prune` cannot read the cache directory

```text
failed to read cache dir /repo/.lattice/cache
```

`lattice prune` treats a missing cache directory as nothing to prune. Any other
read error, such as a permission denial, is fatal.

## Task execution

Raised by the scheduler while running task commands.

### A task failed and the run stopped

```text
task 'app:build' failed, stopping the run
```

The first task failure in a run without `--continue`. No further tasks start,
though any already in flight run to completion. This is the process's fatal
error and the exit code is `1`. The task's own captured output is printed above
this line.

### A task failed and the run kept going

With `--continue`, independent branches keep running and anything downstream of
a failure is skipped rather than started. **No error line prints in this case.**
The run summary carries the counts, and the process exits `1`:

```text
lattice: 2 tasks, 0 cached, 2 failed, 0.01s
```

Search the per-task lines above the summary for `FAILED` to find which tasks
failed. `--sequentially` applies the same rule per phase: a failing phase stops
the remaining phases unless `--continue` is also set, in which case every phase
runs and the process still exits `1`.

### A task's shell could not be spawned

```text
failed to spawn task shell (is `sh` available?): No such file or directory (os error 2)
```

The platform shell, `sh` on unix and `cmd` on Windows, could not be spawned at
all. This is not the task's own command failing, which is an ordinary non-zero
exit. Any spawn error other than "not found" uses a shorter form:

```text
failed to spawn task: <os error>
```

Both are reported as that task's failure.

### A task overran its `timeout`

```text
timed out after 10m and was stopped
```

Reported with the task's captured output, the way any other task failure is. The
task's whole process group is sent `SIGTERM`, given five seconds, then killed.
The duration is rendered in the largest exact unit, so `"timeout": "600s"`
reports `10m`. The task counts as a failure, so the run then stops or carries on
under `--continue` like any other failure. A persistent task has no effective
timeout whatever `timeout` says.

### The run was interrupted

Ctrl-C, or a `SIGTERM` delivered to Lattice, stops the run. Every running task's
process group is sent `SIGTERM`, given five seconds, then killed. **No error line
prints.** The run summary prints and the process exits `130` rather than `1`:

```text
lattice: 1 tasks, 0 cached, 0 failed, 0.00s
```

The tasks that were running are not counted as failures, because the run was
stopped rather than broken. `130` is the shell's convention for `SIGINT`, 128 +
2, which lets a CI runner tell a cancelled job from a failed build.

### The runner panicked

```text
task runner panicked: <panic message>
```

A spawned task's internal work panicked. Fatal regardless of `--continue`, since
the scheduler's own state can no longer be trusted. This is a bug in Lattice
rather than a task failure. Report it.

### `lattice setup` failures

```text
web: `pnpm install` failed
```

A workspace's dependency-install command exited non-zero. A warning per
workspace, so raw output prefixes it with `lattice: warning: `. Setup carries
on to the remaining workspaces. Once every selected workspace has been
attempted, one or more failures fail the command:

```text
setup failed in one or more workspaces
```

Fatal. This is what makes `lattice setup` exit non-zero.

### Two conditions that are not errors

Both print a message and exit `0`. A `--filter` that matches no workspace:

```text
lattice: no workspaces matched filter '<pattern>'.
```

And a repo whose `workspaces` array is empty:

```text
lattice: no workspaces declared. Add one to the `workspaces` array in lattice.json, then run `<tasks>`.
```

`<tasks>` holds the task names you passed, joined by spaces.

## Version pinning and self-update

Raised by `lattice upgrade` and by the automatic handover to a pinned version
that runs before most commands. See [Upgrading](/lattice/docs/upgrading) for the
normal path.

### Not a version

```text
'v1.x' is not a version. Write it like 0.2.0
```

The argument to `lattice upgrade`, or a `latticeVersion` a repo asks to switch
to, did not parse as semver. Fatal. The `Caused by:` line carries the parser's
own complaint, such as `unexpected character 'x' while parsing minor version
number`.

### No downloader available

```text
neither `curl` nor `wget` is on PATH. Install one of them, or download the release archive into .lattice/bin by hand
```

Fetching a release needs one of those two tools, and neither was found. Fatal.

### A downloader was found but would not start

```text
failed to run curl
```

```text
failed to run wget
```

Lattice probed for `curl` and `wget`, found one, and could not launch it. This
is not the tool running and reporting an error, which is [Download or fetch
failure](#download-or-fetch-failure). In practice it is a process limit, or a
tool that left `PATH` mid-run. Fatal, with the OS error under `Caused by:`.

### Download or fetch failure

```text
failed to download https://github.com/latticeandcompany/lattice/releases/download/v0.4.0/lattice-0.4.0-aarch64-apple-darwin.tar.gz: curl: (6) Could not resolve host
```

```text
failed to fetch https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20: curl: (6) Could not resolve host
```

`download` writes an archive to a file. `fetch` reads a response into memory.
Both append the downloader's own stderr. Both fatal.

### Checksum mismatch

```text
checksum mismatch for lattice-0.4.0-aarch64-apple-darwin.tar.gz
  expected 0000000000000000000000000000000000000000000000000000000000000000
  actual   edff58f2a441868dc58c35d06f2b1c86e12e12bedfaa793a49c227672f77566e
Lattice installs only a binary whose checksum matches the published release
```

The downloaded archive's sha256 does not match the release's checksums file.
Fatal, and nothing is installed. A related failure fires when the checksums file
does not cover this platform at all:

```text
lattice-0.4.0-checksums.txt does not list lattice-0.4.0-aarch64-apple-darwin.tar.gz. This platform may have no published build for 0.4.0
```

Also fatal.

### GitHub could not be reached to resolve `latest`

```text
failed to ask GitHub for the newest release
```

`lattice upgrade latest` asks GitHub which release is newest and got no answer:
offline, a DNS failure, a proxy, or a rate limit. Only `latest` needs the
lookup, and a version number is used as given. Fatal, with the `failed to fetch
<url>` detail under `Caused by:`. Retry, or name a version.

### No release to install

```text
no release to install. Tried https://api.github.com/repos/latticeandcompany/lattice/releases/latest and https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20
```

`lattice upgrade latest` reached both endpoints and neither named a release.
Fatal.

### The release archive could not be read or unpacked

```text
failed to open /repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz
```

```text
/repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz is not a readable tar.gz
```

```text
failed to unpack lattice from /repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz
```

Three failures from the same step, after the download and its checksum both
succeeded. The first is a filesystem error opening the archive, and it also
fires one step earlier when hashing the archive for the checksum comparison
cannot open it. The second means the bytes are not a readable gzipped tar, which
a matching checksum does not rule out: a mirror set through
`--release-base-url` can serve a corrupt archive alongside a checksums file that
agrees with it. Damage further into the archive surfaces the tar library's own
message under `Caused by:` instead, such as `unexpected end of file`. The third
means the `lattice` entry was found but writing it into the staging directory
failed, typically a full disk or a read-only `.lattice/bin`. All three fatal.

### The archive holds no binary

```text
/repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz contains no lattice
```

The archive opened and read cleanly but held no `lattice` entry. Fatal. On
Windows the name in the message is `lattice.exe`.

### A version is not installed when linking

```text
/repo/.lattice/bin/lattice-0.4.0 is not installed
```

Lattice was asked to point `.lattice/bin/lattice` at a version-stamped binary
that is not on disk. Fatal. The install step always runs first in normal use, so
this indicates an ordering bug rather than anything you configured.

### An install directory could not be created or written

```text
failed to create /repo/.lattice/bin
```

```text
failed to move the binary into place: /repo/.lattice/bin/.staging-0.4.0-12345/lattice -> /repo/.lattice/bin/lattice-0.4.0
```

The first covers both `.lattice/bin` and the per-process staging directory
underneath it. The second fires after a clean unpack, when moving the extracted
binary to its version-stamped name failed. Both fatal, and both wrap a
filesystem error.

### Linking the stable path failed

```text
failed to create symlink /repo/.lattice/bin/.lattice.link-tmp-12345
```

```text
failed to move symlink into place at /repo/.lattice/bin/lattice
```

The binary installed, but pointing `.lattice/bin/lattice` at it failed. Both
fatal, and both wrap a filesystem error. On Windows the stable path is a copy
rather than a symlink, and the message is `failed to copy <from> to <to>`.

### The automatic handover failed

```text
this repo pins lattice 0.4.0. That version is not installed, and Lattice could not download it.
Run with --no-version-check to use lattice 1.0.0-beta-2 instead
```

A binary Lattice installed under `.lattice/bin` hands the invocation to the
version the repo pins, installing it first if needed. This wraps whatever
install or download error stopped that, so the `Caused by:` block holds one of
the checksum, download, or archive errors above. Fatal unless you pass
`--no-version-check`, set `LATTICE_NO_VERSION_CHECK`, or set
`settings.versionCheck` to `false`, any of which skips the handover so no error
occurs. A binary Lattice did not install prints an advisory nag instead and
changes nothing. See [Environment
variables](/lattice/docs/environment-variables).

### The pinned binary could not be run

```text
failed to run /repo/.lattice/bin/lattice-0.4.0
```

The pinned version installed and linked, and then handing the process over to it
failed. Fatal, with the OS error under `Caused by:`.

### `lattice.json` could not be read or written during `upgrade`

```text
failed to read /repo/lattice.json
```

```text
failed to write /repo/lattice.json
```

`lattice upgrade` reads the file to find the current pin and edits it as text in
place, so key order and formatting survive the bump. Either step can fail on a
filesystem error. Both fatal.
