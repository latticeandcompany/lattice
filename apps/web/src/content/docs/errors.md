---
title: Errors
description: Every error message Lattice emits, what raises it, and how to resolve it.
group: Reference
order: 5
---

# Errors

This page indexes every user-facing error message in Lattice, organized by the
stage of a run that raises it: config loading, workspace discovery, task-graph
construction, toolchain provisioning, cache operations, task execution, and
version self-management. Paste a message you saw into your browser's find
(Ctrl/Cmd-F) to jump to it.

Most errors here are fatal: Lattice prints `Error: <message>` to stderr (an
`anyhow::Error`'s chain adds a `Caused by:` section underneath when one error
wraps another) and exits non-zero. A few are warnings — printed as
`lattice: warning: <message>` and the run continues. Each entry below says
which. For step-by-step fixes to the most common failures, see
[Troubleshooting](/lattice/docs/troubleshooting); for the config fields
mentioned throughout, see [Configuration](/lattice/docs/configuration).

## Config loading and validation

Raised while finding, reading, parsing, and validating `lattice.json`
(`crates/lattice-config/src/lib.rs`, plus root-discovery calls in
`crates/lattice/src/commands/`).

### No `lattice.json` found

```text
no lattice.json found in this directory or any parent; run `lattice init` to create one
```

Raised by `lattice run`, `lattice setup`, and `lattice upgrade` when root
discovery walks up from the working directory and never finds a
`lattice.json`. Fatal. Run `lattice init`, or `cd` into the repo.

`lattice prune` raises the same underlying condition but with a shorter
message that drops the `lattice init` suggestion:

```text
no lattice.json found in this directory or any parent
```

`crates/lattice/src/commands/prune.rs:25`, versus the fuller text at
`crates/lattice/src/commands/run.rs:65-70`, `setup.rs:35-40`, and
`upgrade.rs:30-35`. Worth normalizing in the code so every command suggests
`lattice init` the same way.

### Config file unreadable or unparseable

```text
failed to read lattice.json from /repo
```

```text
failed to parse lattice.json
```

The first wraps a filesystem error (permissions, a directory named
`lattice.json`, and so on); the second wraps the underlying `serde_json` error
(with the line/column) when the file exists but is not valid JSON, or fails to
match the shape of `LatticeConfig`. Both fatal, both from `load_config`
(`crates/lattice-config/src/lib.rs:427-435`).

### Engine declared in unsupported string form

```text
engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not
a well-known engine Lattice can version-check on its own. Use the object form
with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd":
"alpes --version" }
```

Raised by `LatticeConfig::validate` when an `engines` entry (root or
per-workspace) is a bare version string but its name is not in
`WELL_KNOWN_ENGINES` (`crates/lattice-config/src/lib.rs:403-415`; the full list
is on [Toolchains](/lattice/docs/toolchains)). Fatal. Fix by switching to the
object form with an explicit `versionCmd`, exactly as the message shows.

### Workspace has an empty path

```text
workspace 'web' has an empty path
```

The `path` field of a workspace is present but blank or whitespace-only.
Fatal. `crates/lattice-config/src/lib.rs:388`.

### Duplicate workspace name (config-level)

```text
duplicate workspace name 'web': workspace names must be unique
```

Two entries in the `workspaces` array share a `name`. Fatal.
`crates/lattice-config/src/lib.rs:391-395`. (A second, textually different
check for the same condition exists at the discovery stage — see [Duplicate
workspace name (discovery)](#duplicate-workspace-name-or-path-discovery) below.)

### Invalid or missing cache size

Raised by `CacheSize::parse` (`crates/lattice-config/src/lib.rs:242-275`),
used for both `settings.maxCacheSize` in `lattice.json` and `lattice prune
--max-size`:

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

All fatal. Valid units are `B`, `KB`, `MB`, `GB`, `TB` (case-insensitive,
base 1024) or a bare integer of bytes, e.g. `"10GB"` or `"1048576"`.

`lattice prune` also fails when no limit is available at all:

```text
no max cache size set (pass --max-size or set settings.maxCacheSize in lattice.json)
```

Fatal. `crates/lattice/src/commands/prune.rs:34-37`. Pass `--max-size`, or set
`settings.maxCacheSize` in `lattice.json`.

## Workspace discovery and driver detection

Raised while turning the `workspaces` array into resolved `Workspace` values —
checking paths, and picking each workspace's task driver via the evidence
ladder (`crates/lattice-workspace/src/lib.rs`). See [Driver
detection](/lattice/docs/drivers) for the model.

### Ambiguous or undeclared task driver

The evidence ladder's halt condition, `AmbiguityError`
(`crates/lattice-workspace/src/lib.rs:399-430`). Fatal, and it is the only
error whose text is built dynamically from what Lattice actually found in the
workspace directory. Three shapes:

No evidence at all (e.g. only a `.nvmrc`, a runtime with no package manager or
task runner on top of it):

```text
workspace 'app' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "node": ">=0.0.0" }
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

In every case, fix it by pasting the suggested `engines` line into that
workspace's config (or naming whichever tool you actually use). This halt only
fires for a workspace whose `auto` is `true` (the default) — set `auto: false`
to declare `scripts` yourself instead.

### Workspace path is not a directory

```text
workspace path 'apps/gone' does not point to a directory; workspace paths are
literal directories, not globs
```

Raised by `discover_workspaces` when a configured `path` doesn't resolve to an
existing directory (`crates/lattice-workspace/src/lib.rs:812-818`). Fatal. A
common cause is treating `path` as a glob — it never is; see
[Workspaces](/lattice/docs/workspaces).

### Duplicate workspace name or path (discovery)

```text
duplicate workspace name 'web' in lattice.json
```
```text
duplicate workspace path 'apps/web' in lattice.json
```

Raised during discovery (`crates/lattice-workspace/src/lib.rs:821-823`), after
the config-level uniqueness check already described above. The second form
fires even when two workspace entries have different `name`s but the same
resolved directory (after canonicalization). Both fatal.

## Task graph construction

Raised while expanding the root `tasks` map across resolved workspaces into
the execution DAG (`crates/dagger/src/lib.rs`). See [Task
graph](/lattice/docs/task-graph).

### Unknown task

`lattice run` checks every requested task name against `lattice.json` before
building any graph:

```text
task 'lint' is not defined in lattice.json; available tasks: build, test
```

(or `available tasks: (none defined)` when `tasks` is empty). Fatal.
`crates/lattice/src/commands/run.rs:76-89`. `dagger` itself carries an
equivalent, textually different defensive check for library callers that skip
that pre-check:

```text
task 'lint' is not defined in the tasks section of lattice.json
```

`crates/dagger/src/lib.rs:100-105`. Through the CLI you will always see the
first form first.

### Manual workspace declares no command for a requested task

```text
workspace 'legacy' is "auto": false but declares no command for task 'build';
add it under this workspace's "scripts" map in lattice.json
```

A workspace with `auto: false` opts out of all inference — it never invents a
command for a task, so a requested root task with nothing in that workspace's
`scripts` map halts the whole run. (An `auto: true` workspace instead skips
the task quietly if its driver doesn't apply.) Fatal.
`crates/dagger/src/lib.rs:135-142`.

### Persistent task depended on

```text
persistent task 'dev' in workspace 'web' cannot be depended on by other tasks
```

A `persistent: true` task never completes, so nothing may declare it as a
`dependsOn`. Fatal. `crates/dagger/src/lib.rs:193-207`. See [Persistent
tasks](/lattice/docs/persistent-tasks).

### Cycle in the task graph

```text
cycle detected in task dependency graph
```

`dependsOn` edges (same-workspace or `^`-prefixed cross-workspace) form a
cycle, so no topological order exists. Fatal. `crates/dagger/src/lib.rs:210-212`.

## Toolchain resolution and provisioning

Raised while classifying and satisfying each `engines` constraint — host,
validate, or provision (`crates/lattice-workspace/src/toolchain.rs`). See
[Engines and provisioning](/lattice/docs/engines).

### Validate-only failures

For an engine with a version constraint but no `installCmd`:

```text
engine 'alpes': version command `alpes --version` failed:
alpes: command not found
```

The version command exited non-zero (commonly: the tool isn't installed).

```text
engine 'alpes': could not parse version from `alpes --version` output: unknown
```

The version command succeeded but its output had no recognizable number in it
(`parse_version` looks for the first run of digits and dots).

```text
engine 'alpes' has a version constraint but no way to check it (not a
well-known engine and no `versionCmd`)
```

An object-form engine gave a `version` constraint but no `versionCmd`, and the
name isn't well-known enough to have a built-in one. Add `versionCmd`.

```text
engine 'alpes' 1.4.0 on PATH does not satisfy constraint '>=2.0.0'
```

The tool is present and its version parsed, but doesn't satisfy the
constraint. Install/upgrade the tool on `PATH`, or loosen the constraint.

All four fatal. `crates/lattice-workspace/src/toolchain.rs:222-251`.

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

The `installCmd` itself exited non-zero; the combined stdout+stderr is
appended verbatim.

```text
failed to move toolchain into place: /repo/.lattice/toolchains/alpes/tmp-a1b2c3d4 -> /repo/.lattice/toolchains/alpes/1.4.0-a1b2c3d4
```

The install succeeded but renaming the staged directory into its final,
version-stamped path failed (rare; typically a filesystem/permissions issue).

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
constraint — the `installCmd` installed the wrong version.

All five fatal. `crates/lattice-workspace/src/toolchain.rs:258-351`. A rarer,
underlying failure from either mode:

```text
failed to spawn `alpes --version`
```

Wraps an OS error spawning the shell that runs the version/install command
(in practice, `sh` itself missing). `crates/lattice-workspace/src/toolchain.rs:107-109`.

## Cache operations

Raised by cache identity (hashing) and storage (`crates/lattice-cache/src/lib.rs`).
See [Caching](/lattice/docs/caching) for the model and [Cache
internals](/lattice/docs/cache-internals) for the on-disk format.

The load-bearing rule: **a lookup is a hit only if the metadata parses, the
tarball opens, and its sha256 matches the recorded digest.** A missing file,
an unparseable meta, or a corrupted tarball is a miss — never an error, never
a false hit — and the task simply re-runs. Nothing in this section fires on
that path.

### Cache-key computation failure (fatal — fails the task)

```text
failed to compute cache key: failed to read input file src/main.rs
```

Raised inline in the runner before a task starts, if hashing the task's
`inputs` globs fails (usually a file that disappeared mid-glob) — wraps
`compute_key`'s own errors:

```text
failed to read input file <path>
```
```text
failed to read lockfile <path>
```

Both from `crates/lattice-cache/src/lib.rs:376-390`. This is the one cache
error that fails the task outright (`crates/lattice-runner/src/lib.rs:518-536`);
every other cache failure below is a non-fatal warning.

### Read/write warnings (non-fatal)

Printed as `lattice: warning: <message>` by the reporter and otherwise
ignored:

```text
app:build: cache lookup failed: failed to parse cache metadata at /repo/.lattice/cache/<key>.meta.json
```
```text
app:build: failed to restore cached outputs: failed to unpack artifact into /repo/apps/app
```
```text
app:build: failed to cache outputs: failed to create cache dir /repo/.lattice/cache
```

These wrap `LocalStore`'s own errors — `failed to parse cache metadata at
<path>`, `failed to read <path>`, `failed to write cache metadata at <path>`,
`failed to create cache dir <path>`, `failed to create artifact <path>`,
`failed to add <path> to artifact`, `failed to digest artifact <path>`,
`failed to open artifact <path>`, `failed to unpack artifact into <path>`
(`crates/lattice-cache/src/lib.rs:99-108`). A lookup failure or a restore
failure both fall through to running the task fresh; a store failure just
warns after a successful run. None of these fail the build.

### `prune` cache-directory read failure (fatal)

```text
failed to read cache dir /repo/.lattice/cache
```

`LocalStore::prune` treats a missing cache directory as nothing to prune, but
any other read error (permissions) is fatal. `crates/lattice-cache/src/lib.rs:246-251`.

## Task execution

Raised by the scheduler while actually running task commands
(`crates/lattice-runner/src/lib.rs`).

### A task failed (fail-fast — the default)

```text
task 'app:build' failed, stopping pipeline
```

The first task failure in a run without `--continue` stops scheduling new
work; already-running tasks finish, then this is the process's fatal error and
the exit code is non-zero. `crates/lattice-runner/src/lib.rs:417-431`. The
task's own captured stdout/stderr is surfaced above this line.

### One or more tasks failed (`--continue` / keep-going)

```text
2 tasks failed (kept going); 3 downstream tasks skipped
```

With `--continue`, independent branches keep running after a failure; anything
downstream of the failure is skipped rather than started. This summary is the
final error (`RunFailure`, `crates/lattice-runner/src/lib.rs:44-70`), and the
run still exits non-zero.

### Task command failed to spawn

```text
failed to spawn task shell (is `sh` available?): No such file or directory (os error 2)
```

The platform shell (`sh` on unix, `cmd` on Windows) could not be spawned at
all — distinct from the task's own command failing, which is a normal
non-zero exit and not this message. `crates/lattice-runner/src/lib.rs:597-613`.
Any other spawn error (not "not found") uses a shorter form:

```text
failed to spawn task: <os error>
```

### Runner panic (fatal even with `--continue`)

```text
task runner panicked: <panic message>
```

A spawned task's async work panicked — a Lattice-internal fault, not a task
failure. Treated as fatal regardless of `--continue`, since the scheduler's own
state can no longer be trusted. `crates/lattice-runner/src/lib.rs:388-397`. If
you see this, it's worth filing: it means a bug in Lattice, not your task.

### `lattice setup` failures

```text
web: `pnpm install` failed
```

A workspace's dependency-install command exited non-zero; printed as a
warning per workspace, and setup continues to the remaining workspaces.
`crates/lattice/src/commands/setup.rs:144-151`. After all workspaces are
attempted, if any failed:

```text
one or more workspaces failed setup
```

Fatal — this is what makes `lattice setup`'s exit code non-zero.
`crates/lattice/src/commands/setup.rs:154-156`.

Two related conditions from `lattice run` are **not** errors — they print a
message and exit 0: a `--filter` that matches no workspace
(`lattice: no workspaces matched filter '<pattern>'.`), and a repo with an
empty `workspaces` array (`lattice: no workspaces declared. Add them to the
`workspaces` array in lattice.json to run `<tasks>`.`). Both in
`crates/lattice/src/commands/run.rs:105-125`.

## Version pinning and self-update

Raised by `lattice upgrade` and by the automatic version-pin handover that
runs before most commands (`crates/lattice/src/release.rs`,
`crates/lattice/src/drift.rs`, `crates/lattice/src/commands/upgrade.rs`). See
[Upgrading](/lattice/docs/upgrading) for the normal-path behavior of
`latticeVersion` pinning and the drift nag.

### Not a version

```text
'v1.x' is not a version (expected something like 0.2.0)
```

`lattice upgrade <version>` (or the pinned `latticeVersion` a repo asks to
switch to) didn't parse as semver. Fatal. `crates/lattice/src/release.rs:53-59`.

### No fetcher available

```text
neither `curl` nor `wget` is on PATH; install one, or download the release
archive by hand into .lattice/bin
```

Fetching a release (for `lattice upgrade`, or the automatic handover to a
pinned version) needs one of these two tools; neither was found. Fatal.
`crates/lattice/src/release.rs:367-368`.

### Download or fetch failure

```text
failed to download https://github.com/latticeandcompany/lattice/releases/download/v0.4.0/lattice-0.4.0-aarch64-apple-darwin.tar.gz: curl: (6) Could not resolve host
```
```text
failed to fetch https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20: curl: (6) Could not resolve host
```

Both wrap the fetcher's own stderr. Fatal. `crates/lattice/src/release.rs:370-416`.

### Checksum mismatch

```text
checksum mismatch for lattice-0.4.0-aarch64-apple-darwin.tar.gz
  expected 9f2c...
  actual   1a0b...
refusing to install a binary that does not match the published release
```

The downloaded archive's sha256 doesn't match the release's checksums file.
Fatal, and deliberate — Lattice refuses to install it.
`crates/lattice/src/release.rs:177-183`. A related, rarer failure when the
checksums file itself doesn't cover this platform:

```text
lattice-0.4.0-checksums.txt does not list lattice-0.4.0-aarch64-apple-darwin.tar.gz; this platform may not be published for 0.4.0
```

`crates/lattice/src/release.rs:171-176`.

### No release to install

```text
no release to install; tried https://api.github.com/repos/latticeandcompany/lattice/releases/latest and https://api.github.com/repos/latticeandcompany/lattice/releases?per_page=20
```

`lattice upgrade latest` found nothing published at either endpoint. Fatal.
`crates/lattice/src/release.rs:264-268`.

### Archive has no binary

```text
/repo/.lattice/bin/.staging-0.4.0-12345/lattice-0.4.0-aarch64-apple-darwin.tar.gz contains no lattice
```

The downloaded archive was read but had no `lattice` entry inside. Fatal.
`crates/lattice/src/release.rs:354`.

### A version isn't installed when linking

```text
/repo/.lattice/bin/lattice-0.4.0 is not installed
```

`link_stable` was asked to point the stable `lattice` symlink at a
version-stamped binary that isn't on disk. Fatal.
`crates/lattice/src/release.rs:208`. In normal use `ensure_installed` always
runs first, so this indicates an internal ordering bug if you see it.

### Automatic handover failed

```text
this repo pins lattice 0.4.0, which is not installed and could not be fetched.
Run with --no-version-check to use lattice 0.3.1 anyway
```

Any command run in a repo whose `latticeVersion` differs from the invoked
binary tries to install and switch to the pinned version first
(`crates/lattice/src/drift.rs:99-136`); this wraps whatever
`ensure_installed`/`release` error occurred (see the checksum, fetch, and
fetcher errors above) with the escape hatch spelled out. Fatal unless you pass
`--no-version-check` or set `LATTICE_NO_VERSION_CHECK` (see [Environment
variables](/lattice/docs/environment-variables)), in which case the handover
is skipped entirely and no error occurs.

### `lattice.json` unreadable/unwritable during `upgrade`

```text
failed to read /repo/lattice.json
```
```text
failed to write /repo/lattice.json
```

`lattice upgrade` reads the file to find the current pin and text-edits it in
place to avoid reformatting the rest of the file; either step can fail on a
filesystem error. Fatal. `crates/lattice/src/commands/upgrade.rs:38-39,72-74`.
