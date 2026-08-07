---
title: Errors
description: Every error message Lattice emits, what raises it, and how to resolve it.
group: Reference
order: 5
---

# Errors

Every user-facing error message in Lattice, grouped by the stage of a run that
raises it. Use your browser's find (Ctrl/Cmd-F) on a message you saw.

Most are fatal: Lattice prints `Error: <message>` to stderr and exits non-zero.
A `Caused by:` section appears underneath when one error wraps another. A few are
warnings — printed as `lattice: warning: <message>`, and the run continues. Each
entry says which. For step-by-step fixes to the most common failures, see
[Troubleshooting](/lattice/docs/troubleshooting); for the config fields mentioned
throughout, see [Configuration](/lattice/docs/configuration).

## Config loading and validation

Raised while finding, reading, parsing, and validating `lattice.json`.

### No `lattice.json` found

```text
no lattice.json found in this directory or any parent; run `lattice init` to create one
```

Raised by `lattice run`, `lattice setup`, `lattice prune`, and `lattice upgrade`
when root discovery walks up from the working directory and never finds a
`lattice.json`. All four emit the same text. Fatal. Run `lattice init`, or `cd`
into the repo.

### Config file unreadable or unparseable

```text
failed to read lattice.json from /repo
```

```text
failed to parse lattice.json
```

The first wraps a filesystem error (permissions, a directory named
`lattice.json`, and so on). The second reports the JSON parse failure, with line
and column, when the file exists but is not valid JSON, or doesn't match the
config schema. Both fatal.

### Unknown key in `lattice.json`

```text
unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: dependsOn, inputs, outputs, ignore, env, persistent, cache
```

Raised while parsing, for a key that is not part of the config at that level. The
first line names the key, the object holding it (`workspaces[1]`, `tasks.build`,
`engines.node`, or `at the top level of lattice.json`), and where it is in the
file. `Did you mean` appears when a valid field is within a character or two.
Fatal, and raised before any workspace is read. Delete the key, or correct it to
the field the message names. See
[Configuration](/lattice/docs/configuration#unknown-keys).

### Engine value in neither accepted form

```text
invalid type: integer `20`, expected a version constraint string or an engine object
```

An `engines` entry whose value is neither a version-constraint string
(`">=20.0.0"`) nor an object (`{ "version": ">=20.0.0" }`). Fatal.

### Engine declared in unsupported string form

```text
engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not
a well-known engine Lattice can version-check on its own. Use the object form
with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd":
"alpes --version" }
```

Raised during config validation when an `engines` entry (root or per-workspace)
is a bare version string but its name is not on the well-known engine list (that
list is on [Toolchains](/lattice/docs/toolchains)). Fatal. Switch to the object
form with an explicit `versionCmd`, as the message shows.

### Workspace has an empty path

```text
workspace 'web' has an empty path
```

The `path` field of a workspace is present but blank or whitespace-only. Fatal.

### Workspace path points outside the repo

```text
workspace 'esc' has a path '../outside' that points outside the repo root;
workspace paths must stay inside the repo
```

The `path` is absolute, or climbs above the repo root with `..`. Fatal. The
workspace directory bounds which files are hashed, which the `outputs` globs
match, and which a cache hit clears before unpacking, so it has to be somewhere
inside the repo.

### Workspace `dependsOn` names an undeclared workspace

```text
workspace 'api' depends on 'cor', which is not a declared workspace
Did you mean `core`?
Declared workspaces: core, api
```

A name in a workspace's `dependsOn` matches no entry in `workspaces`. Fatal. An
unresolvable name builds no edge, so `^task` would expand to nothing and the
ordering the config was written to guarantee would silently not happen. The
nearest declared name is offered when one is close enough to be a typo. A
workspace listing itself is rejected the same way.

### Task `dependsOn` names an undefined task

```text
task 'build' depends on 'codegen', but 'codegen' is not defined in `tasks`
Did you mean `codegin`?
Defined tasks: build, test, codegin
```

A name in a task's `dependsOn` has no entry in the `tasks` map. Fatal, for the
same reason as above: the prerequisite would resolve to no node and simply not
run. The `^` prefix is stripped before the check, so `^build` is checked against
`build`.

### Duplicate workspace name (config-level)

```text
duplicate workspace name 'web': workspace names must be unique
```

Two entries in the `workspaces` array share a `name`. Fatal. A second, textually
different check for the same condition exists at the discovery stage — see
[Duplicate workspace name or path
(discovery)](#duplicate-workspace-name-or-path-discovery).

### Invalid or missing cache size

Raised while parsing a cache size, which happens for both
`settings.maxCacheSize` in `lattice.json` and `lattice prune --max-size`:

```text
cache size is empty
```
```text
cache size '10XB' has no numeric component
```
```text
invalid numeric component in cache size '10..5GB'
```
```text
unknown cache size unit 'XB' in '10XB'
```

All fatal. Valid units are `B`, `KB`, `MB`, `GB`, `TB` (case-insensitive, base
1024) or a bare integer of bytes, e.g. `"10GB"` or `"1048576"`.

`lattice prune` also fails when no limit is available at all:

```text
no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)
```

Fatal. Pass `--max-size`, or set `settings.maxCacheSize`.

## Workspace discovery and driver detection

Raised while resolving the `workspaces` array — checking paths, and picking each
workspace's task driver via the evidence ladder. See [Driver
detection](/lattice/docs/drivers) for the model.

### Ambiguous or undeclared task driver

The evidence ladder's halt condition. Fatal. Its text is built from what Lattice
found in the workspace directory, so it takes three shapes.

No evidence at all (e.g. only a `.nvmrc`, a runtime with no package manager or
task runner on top of it):

```text
workspace 'app' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "auto": false, "scripts": { "build": "<command>" }
```

A bare generic ecosystem marker (e.g. only `package.json`, no lockfile):

```text
workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: pnpm, npm, yarn, bun
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "pnpm": ">=0.0.0" }
```

Two same-role tools present at once (e.g. both `bun.lockb` and
`pnpm-lock.yaml`):

```text
workspace 'web' has an ambiguous or undeclared task driver.
Candidate tools seen: bun, pnpm
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "bun": ">=0.0.0" }
```

Fix it by pasting the suggested line into that workspace's config, or by naming
whichever tool you actually use. The suggestion is always one that resolves the
halt: an `engines` entry where a candidate tool can drive tasks, and the
`scripts` form where none can, since a runtime alone never can. This halt only
fires for a workspace whose `auto` is `true` (the default).

### Workspace path is not a directory

```text
workspace path 'apps/gone' does not point to a directory; workspace paths are
literal directories, not globs
```

Raised during workspace discovery when a configured `path` doesn't resolve to an
existing directory. Fatal. `path` is never a glob; see
[Workspaces](/lattice/docs/workspaces).

### Duplicate workspace name or path (discovery)

```text
duplicate workspace name 'web' in lattice.json
```
```text
duplicate workspace path 'apps/web' in lattice.json
```

Raised during discovery, after the config-level uniqueness check above. The
second form fires even when two workspace entries have different `name`s but the
same resolved directory, after canonicalization. Both fatal.

## Task graph construction

Raised while expanding the root `tasks` map across resolved workspaces into the
execution DAG. See [Task graph](/lattice/docs/task-graph).

### Unknown task

`lattice run` checks every requested task name against `lattice.json` before
building any graph:

```text
task 'lint' is not defined in lattice.json; available tasks: build, test
```

(or `available tasks: (none defined)` when `tasks` is empty). Fatal. Graph
construction carries an equivalent, textually different check for callers that
skip that pre-check:

```text
task 'lint' is not defined in the tasks section of lattice.json
```

Through the CLI you always see the first form first.

### Manual workspace declares no command for a requested task

```text
workspace 'legacy' is "auto": false but declares no command for task 'build';
add it under this workspace's "scripts" map in lattice.json
```

A workspace with `auto: false` opts out of all inference and never invents a
command, so a requested root task with nothing in that workspace's `scripts` map
halts the whole run. An `auto: true` workspace instead skips the task quietly if
its driver doesn't apply. Fatal. A `--filter`ed run only holds the workspaces it
matched to this check: one pulled in as a dependency is asked for the tasks its
dependents need, not for the task you named.

### Persistent task depended on

```text
persistent task 'dev' in workspace 'web' cannot be depended on by other tasks
```

A `persistent: true` task never completes, so nothing may declare it as a
`dependsOn`. Fatal. See [Persistent tasks](/lattice/docs/persistent-tasks).

### Cycle in the task graph

```text
cycle detected in task dependency graph
```

`dependsOn` edges (same-workspace or `^`-prefixed cross-workspace) form a cycle,
so no topological order exists. Fatal.

## Toolchain resolution and provisioning

Raised while classifying and satisfying each `engines` constraint — host,
validate, or provision. See [Engines and provisioning](/lattice/docs/engines).

### Validate-only failures

For an engine with a version constraint but no `installCmd`:

```text
engine 'alpes': version command `alpes --version` failed:
alpes: command not found
```

The version command exited non-zero; commonly, the tool isn't installed.

```text
engine 'alpes': could not parse version from `alpes --version` output: unknown
```

The version command succeeded but its output had no recognizable number in it.
Lattice takes the first run of digits and dots it finds.

```text
engine 'alpes' has a version constraint but no way to check it (not a
well-known engine and no `versionCmd`)
```

An object-form engine gave a `version` constraint but no `versionCmd`, and the
name isn't well-known enough to have a built-in one. Add `versionCmd`.

```text
engine 'alpes' 1.4.0 on PATH does not satisfy constraint '>=2.0.0'
```

The tool is present and its version parsed, but doesn't satisfy the constraint.
Install or upgrade the tool on `PATH`, or loosen the constraint.

All four fatal.

### Provisioning failures

For an engine with an `installCmd`:

```text
failed to create toolchain dir /repo/.lattice/toolchains/alpes
```

Wraps a filesystem error creating the content-addressed install directory.

```text
engine 'alpes': installCmd failed:
curl: (6) Could not resolve host: alpes.example
```

The `installCmd` itself exited non-zero; the combined stdout+stderr is appended
verbatim.

```text
failed to move toolchain into place: /repo/.lattice/toolchains/alpes/tmp-a1b2c3d4 -> /repo/.lattice/toolchains/alpes/1.4.0-a1b2c3d4
```

The install succeeded but renaming the staged directory into its final,
version-stamped path failed. Typically a filesystem or permissions issue.

```text
engine 'alpes': version command `alpes --version` failed after install:
alpes: command not found
```

The install command exited 0, but the version command against the freshly
installed `bin` dir still failed — often a wrong `bin` path in the engine spec.

```text
engine 'alpes' provisioned 1.4.0 does not satisfy '>=2.0.0'
```

The freshly installed tool's own version doesn't satisfy the declared
constraint: the `installCmd` installed the wrong version.

All five fatal. A rarer failure underlying either mode:

```text
failed to spawn `alpes --version`
```

Wraps an OS error spawning the shell that runs the version or install command —
in practice, `sh` itself missing.

## Cache operations

Raised by cache identity (hashing) and cache storage. See
[Caching](/lattice/docs/caching) for the model and [Cache
internals](/lattice/docs/cache-internals) for the on-disk format.

A lookup is a hit only if the metadata parses, the tarball opens, and its sha256
matches the recorded digest. A missing file, an unparseable meta, or a corrupted
tarball is a miss, and the task re-runs. Nothing in this section fires on that
path.

### Cache-key computation failure (fatal — fails the task)

```text
failed to compute cache key: failed to read input file src/main.rs
```

Raised before a task starts, if hashing the task's `inputs` globs fails — usually
a file that disappeared mid-glob. It wraps whichever underlying read failed:

```text
failed to read input file <path>
```
```text
failed to read lockfile <path>
```

This is the one cache error that fails the task outright; every other cache
failure below is a non-fatal warning.

### Read/write warnings (non-fatal)

Printed as `lattice: warning: <message>` and otherwise ignored:

```text
app:build: cache lookup failed: failed to parse cache metadata at /repo/.lattice/cache/<key>.meta.json
```
```text
app:build: failed to restore cached outputs: failed to unpack artifact into /repo/apps/app
```
```text
app:build: failed to cache outputs: failed to create cache dir /repo/.lattice/cache
```

Each wraps one of the underlying cache-storage errors: `failed to parse cache
metadata at <path>`, `failed to read <path>`, `failed to write cache metadata at
<path>`, `failed to create cache dir <path>`, `failed to create artifact <path>`,
`failed to add <path> to artifact`, `failed to digest artifact <path>`, `failed to
open artifact <path>`, `failed to unpack artifact into <path>`. A lookup failure
or a restore failure falls through to running the task fresh; a store failure
warns after a successful run. None fail the build.

### `prune` cache-directory read failure (fatal)

```text
failed to read cache dir /repo/.lattice/cache
```

`lattice prune` treats a missing cache directory as nothing to prune. Any other
read error, such as permissions, is fatal.

## Task execution

Raised by the scheduler while running task commands.

### A task failed (fail-fast — the default)

```text
task 'app:build' failed, stopping pipeline
```

The first task failure in a run without `--continue` stops scheduling new work.
Already-running tasks finish, then this is the process's fatal error and the exit
code is non-zero. The task's own captured stdout/stderr is surfaced above this
line.

### One or more tasks failed (`--continue` / keep-going)

```text
2 tasks failed (kept going); 3 downstream tasks skipped
```

With `--continue`, independent branches keep running after a failure; anything
downstream of the failure is skipped rather than started. This summary is the
run's final error, and the run exits non-zero.

### Task command failed to spawn

```text
failed to spawn task shell (is `sh` available?): No such file or directory (os error 2)
```

The platform shell (`sh` on unix, `cmd` on Windows) could not be spawned at all.
Distinct from the task's own command failing, which is a normal non-zero exit and
not this message. Any other spawn error (not "not found") uses a shorter form:

```text
failed to spawn task: <os error>
```

### A task overran its `timeout`

```text
timed out after 10m and was stopped
```

Surfaced with the task's captured output, the way any other task failure is. The
task's whole process group was sent `SIGTERM`, given five seconds, then killed.
The task counts as a failure, so the run then behaves as it does for any
failure: it stops the pipeline, or carries on under `--continue`.

### The run was interrupted

```text
interrupted — running tasks were stopped
```

Ctrl-C or a `SIGTERM` reached Lattice. Every running task's process group is sent
`SIGTERM`, given five seconds, then killed, and the process exits `130` rather
than `1`. The tasks that were running did not fail; the run was stopped, and the
message says so rather than reporting them as failures.

### Runner panic (fatal even with `--continue`)

```text
task runner panicked: <panic message>
```

A spawned task's internal work panicked. Treated as fatal regardless of
`--continue`, since the scheduler's own state can no longer be trusted. This is a
bug in Lattice, not a task failure; report it.

### `lattice setup` failures

```text
web: `pnpm install` failed
```

A workspace's dependency-install command exited non-zero. Printed as a warning
per workspace, and setup continues to the remaining workspaces. After all
workspaces are attempted, if any failed:

```text
one or more workspaces failed setup
```

Fatal — this is what makes `lattice setup`'s exit code non-zero.

Two related conditions from `lattice run` are **not** errors; they print a
message and exit 0. A `--filter` that matches no workspace:

```text
lattice: no workspaces matched filter '<pattern>'.
```

And a repo with an empty `workspaces` array:

```text
lattice: no workspaces declared. Add them to the `workspaces` array in lattice.json to run `<tasks>`.
```

## Version pinning and self-update

Raised by `lattice upgrade` and by the automatic version-pin handover that runs
before most commands. See [Upgrading](/lattice/docs/upgrading) for the normal-path
behavior of `latticeVersion` pinning and the drift nag.

### Not a version

```text
'v1.x' is not a version (expected something like 0.2.0)
```

`lattice upgrade <version>` (or the pinned `latticeVersion` a repo asks to switch
to) didn't parse as semver. Fatal.

### No fetcher available

```text
neither `curl` nor `wget` is on PATH; install one, or download the release
archive by hand into .lattice/bin
```

Fetching a release — for `lattice upgrade`, or the automatic handover to a pinned
version — needs one of these two tools; neither was found. Fatal.

### Fetcher found but failed to start

```text
failed to run curl
```
```text
failed to run wget
```

Lattice probed for `curl` and `wget`, found one, and then could not launch it.
Distinct from the tool running and reporting an error, which is [Download or
fetch failure](#download-or-fetch-failure). In practice this is a process limit
or a tool that left `PATH` mid-run. Fatal; the `Caused by:` line carries the
underlying OS error.

### Download or fetch failure

```text
failed to download https://github.com/latticeandcompany/lattice/releases/download/v0.4.0/lattice-0.4.0-aarch64-apple-darwin.tar.gz: curl: (6) Could not resolve host
```
```text
failed to fetch https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20: curl: (6) Could not resolve host
```

Both wrap the fetcher's own stderr. Fatal.

### Checksum mismatch

```text
checksum mismatch for lattice-0.4.0-aarch64-apple-darwin.tar.gz
  expected 9f2c...
  actual   1a0b...
refusing to install a binary that does not match the published release
```

The downloaded archive's sha256 doesn't match the release's checksums file.
Fatal. A rarer related failure, when the checksums file itself doesn't cover this
platform:

```text
lattice-0.4.0-checksums.txt does not list lattice-0.4.0-aarch64-apple-darwin.tar.gz; this platform may not be published for 0.4.0
```

Also fatal.

### Could not reach GitHub to resolve `latest`

```text
failed to ask GitHub for the newest release
```

`lattice upgrade latest` asks GitHub which release is newest and got no answer —
offline, DNS failure, a proxy in the way, or a rate limit. Only `latest` needs
this lookup; a bare version number is used as given. Fatal; the `Caused by:` line
carries the `failed to fetch <url>` detail. Retry, or name an explicit version.

### No release to install

```text
no release to install; tried https://api.github.com/repos/latticeandcompany/lattice/releases/latest and https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20
```

`lattice upgrade latest` reached GitHub but found nothing published at either
endpoint. Fatal.

### Release archive could not be read or unpacked

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
succeeded. The first is a filesystem error opening the archive; it also fires
one step earlier, when hashing the archive for the checksum comparison cannot
open it. The second means the bytes are not a valid gzipped tar, which a matching
checksum does not rule out — a mirror set through `--release-base-url` can serve
a corrupt archive alongside a checksums file that agrees with it. The third means
the `lattice` entry was found but writing it into the staging directory failed,
typically a full disk or a read-only `.lattice/bin`. All three fatal.

### Archive has no binary

```text
/repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz contains no lattice
```

The archive opened and read cleanly but had no `lattice` entry inside. Fatal.

### A version isn't installed when linking

```text
/repo/.lattice/bin/lattice-0.4.0 is not installed
```

Lattice was asked to point the stable `.lattice/bin/lattice` symlink at a
version-stamped binary that isn't on disk. Fatal. The install step always runs
first in normal use, so this indicates an internal ordering bug.

### Automatic handover failed

```text
this repo pins lattice 0.4.0, which is not installed and could not be fetched.
Run with --no-version-check to use lattice 0.3.1 anyway
```

Any command run in a repo whose `latticeVersion` differs from the invoked binary
tries to install and switch to the pinned version first. This wraps whatever
install or download error occurred — see the checksum, fetch, and fetcher errors
above. Fatal unless you pass `--no-version-check` or set
`LATTICE_NO_VERSION_CHECK` (see [Environment
variables](/lattice/docs/environment-variables)), in which case the handover is
skipped entirely and no error occurs.

### `lattice.json` unreadable/unwritable during `upgrade`

```text
failed to read /repo/lattice.json
```
```text
failed to write /repo/lattice.json
```

`lattice upgrade` reads the file to find the current pin and text-edits it in
place, so key order and formatting survive a bump. Either step can fail on a
filesystem error. Fatal.
