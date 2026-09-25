---
title: Errors
description: Every error message Lattice emits, what raises it, and how to resolve it.
group: Reference
order: 5
---

# Errors

Every message Lattice fails on, plus the warnings it prints and the messages
that read like failures and are not. Entries are grouped by the stage of a run
that raises them. Each message appears the way the program prints it, so a find
in this page matches what you just saw in your terminal.

A fatal error goes to stderr as `error: ` followed by the message, and the
process exits `1`. When one error wraps another, each cause follows on its own
indented line. A warning does not stop the run: the stream prefixes it with
`lattice: warning: `, adding `<workspace>:<task>: ` when it belongs to one task,
and the live display prints it after a yellow `!`.

For symptom-first fixes, see [Troubleshooting](/lattice/docs/troubleshooting).
For the fields these messages name, see
[Configuration](/lattice/docs/configuration). For the exit codes, see the [CLI
reference](/lattice/docs/cli#exit-codes).

## Config loading and validation

Raised while finding, reading, parsing and validating `lattice.json`. Nothing
has run.

### No `lattice.json` found

```text
no lattice.json found in this directory or any parent. Run `lattice init` to create one
```

Every command walks up from the current directory looking for the file.

### A 1.x config

```text
lattice.json is written for Lattice 1.x, which listed every workspace as an object with a name and a path. Run `lattice migrate` to rewrite it for this version
```

The config's `workspaces` holds objects. Run `lattice migrate`. See [Upgrading to
2.0](/lattice/docs/upgrading#upgrading-to-20).

### Config file unreadable or unparseable

```text
failed to parse lattice.json
  expected `,` or `}` at line 4 column 3
```

The file is not valid JSON. The indented line is the parser's message and
position.

### Unknown key in `lattice.json`

```text
unknown field `output` in tasks.build (lattice.json line 5, column 14)
Did you mean `outputs`?
Fields accepted here: run, dependsOn, inputs, outputs, ignore, env, persistent, cache, timeout
```

A key Lattice does not define, at any level. The message names the object
holding it (`tasks.build`, `overrides.web`, `engines.node`, `settings`, or `at
the top level of lattice.json`), the position, the nearest valid field when one
is within one or two edits, and every field accepted there. `output` for
`outputs` is why this is fatal: the typo would silently change what the task
caches.

### The same key written twice

```text
duplicate key `build` in tasks (lattice.json line 8, column 3)
Keep one of them: the second replaces the first, so only the last would take effect
```

Raised for `tasks`, `engines`, `overrides`, an override's `engines`, and an
override's `tasks`. The position is where the object closes. A field repeated on
a fixed object is reported by the parser instead:

```text
failed to parse lattice.json
  duplicate field `name` at line 3 column 20
```

### Engine value in neither accepted form

```text
failed to parse lattice.json
  invalid type: integer `20`, expected a version constraint string or an engine object at line 1 column 25
```

### Engine declared in the string form Lattice cannot check

```text
engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
```

The scope reads `root` or `overrides.<key>`. The 40 names that do work are on
[Toolchains](/lattice/docs/toolchains#well-known-engines).

### Engine `bin` does not stay inside the toolchain

```text
engine 'node' in overrides.web has a `bin` of '/usr/bin', which is not relative to the toolchain install. Write `bin` as a path inside the toolchain directory, like "bin"
```

Siblings: `... which points outside the toolchain install`, `... has an empty
\`bin\``, and one for whitespace around a directory name.

### A `workspaces` or `exclude` entry outside the repo

```text
`workspaces` entry '../x' points outside the repo root. Every workspace has to be inside the repo
```

```text
`workspaces` entry '/srv/app' is not relative to the repo root. Write it relative to the repo root, like "apps/*"
```

```text
`workspaces` entry 'apps/ web' has leading or trailing whitespace around a directory name. Windows drops that whitespace and unix keeps it, so the entry would match a different directory on each. Remove it
```

```text
`workspaces` entry is empty. Remove it, or write a directory like "apps/*"
```

The same wording with `` `exclude` `` for an `exclude` entry.

### `run` in the root `tasks` map

```text
`tasks.build` has a `run`, but the root `tasks` map says how a task behaves in every workspace, not what it runs. Put the command under the workspace it belongs to: "overrides": { "<workspace>": { "tasks": { "build": "<command>" } } }
```

### A task that depends on itself

```text
task 'build' lists itself in `dependsOn`
```

### An empty override name

```text
`overrides.web.name` is empty. Give the workspace a name, or leave the key out
```

### `settings.cacheDir` does not name a usable directory

```text
`settings.cacheDir` is '.', which is the repo root itself. Point it at a directory of its own, like ".lattice/cache" — `lattice prune` deletes cache archives and partial writes in whatever directory it names
```

Siblings for an empty value, an absolute path, one outside the repo, and
whitespace around a directory name.

### Invalid cache size

```text
failed to parse lattice.json
  invalid type: integer `1048576`, expected a string at line 1 column 35
```

```text
failed to parse lattice.json
  unknown cache size unit 'GIGS' in '10 gigs'. Use B, KB, MB, GB, or TB at line 1 column 44
```

### Invalid duration

```text
failed to parse lattice.json
  duration '99999h' is longer than the maximum of 365 days. Use a shorter duration, or leave `timeout` out to let the task run without a limit at line 1 column 37
```

```text
failed to parse lattice.json
  duration 1.5 is not a whole number of seconds. Write a whole number of seconds, or a duration string such as "90s", "1500ms", or "10m" at line 1 column 32
```

Siblings: `duration '<value>' does not start with a number`, `unknown duration
unit '<unit>' in '<value>'. Use ms, s, m, or h`, `duration '<value>' must be
greater than zero`, and `duration of <n> seconds is longer than the maximum of
365 days`.

### `prune` has no limit to work to

```text
no cache size limit set. Pass --max-size, or set settings.maxCacheSize in lattice.json
```

## Workspace discovery and driver detection

Raised while finding workspaces, picking each one's tool and working out
dependencies. Nothing has run.

### A `workspaces` entry matches nothing

```text
`workspaces` entry 'tolls/*' matches no directory. Check the spelling, or remove the entry
```

### An invalid glob

```text
`workspaces` entry 'a/[' is not a valid glob
  error parsing glob 'a/[': unclosed character class; missing ']'
```

The same for `exclude` entries and `overrides` keys.

### Lattice cannot tell which tool runs a workspace

```text
Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

A `package.json` or `pyproject.toml` workspace with no lockfile, no
`packageManager` field and no `[tool.*]` table, in its own directory or any
directory above it — or two tools tied on equal evidence. Name the tool the
workspace uses, or commit its lockfile. The suggestion is only the first
candidate. See [Driver detection](/lattice/docs/drivers).

```text
Lattice cannot tell what runs the tasks in workspace 'tools': it has no lockfile, wrapper or manifest Lattice recognizes.
Say which in lattice.json:
  "overrides": { "tools": { "tasks": { "build": "<command>" } } }
```

A member a root workspace file names that holds nothing Lattice recognizes. A
directory a `lattice.json` glob matched is skipped silently instead.

### An override key matches no workspace

```text
`overrides` key 'weeb' names no workspace
Did you mean `web`?
Workspaces: api, web
```

A key with no `/` or glob character is a workspace name, and names come from
manifests: `packages/ui` may be `@acme/ui`. A path key that matches nothing
reads `` `overrides` key 'app/*' matches no workspace path ``.

### An unknown driver

```text
workspace 'gen' names driver 'pnmp', which Lattice does not know
Did you mean `pnpm`?
Use one of node, deno, bun, pnpm, yarn, npm, cargo, go, uv, poetry, pdm, pipenv, pip, python, bundler, rake, ruby, gradle, maven, java, kotlin, dotnet, nuget, pod, swift, composer, mix, dart, stack, cabal, just, task, turbo, nx, or "none" and write each task's command under `tasks`
```

### A dependency on a workspace that does not exist

```text
workspace 'web' depends on 'apii', which is not a workspace
Did you mean `api`?
Workspaces: api, web
```

An override's `dependsOn` naming no workspace. An unresolvable name would build
no edge, and the run would lose its ordering without a word.

```text
workspace 'web' lists itself in `dependsOn`
```

### Two workspaces with one name

```text
two workspaces are named 'web'. Give each a different `name` in `overrides`
```

Two derived names that collide are renamed to their paths automatically; this
fires only when an override chose the name.

### A workspace outside the repo

```text
workspace 'app' resolves to /elsewhere/app, which is outside the repo root. A workspace directory has to be inside the repo
```

The directory is a symlink out of the repo.

### A manifest could not be read

```text
web: package.json could not be parsed: expected `,` or `}` at line 4 column 3, so it runs none of its scripts
```

A warning. The workspace runs none of its scripts until the file is fixed.
Siblings: `<file> could not be read: <error>` and `<file> has a "scripts" that is
not an object` (`"tasks"` for deno). `deno.json`, `deno.jsonc` and `turbo.json`
may carry comments; `package.json` may not.

## Task graph construction

Raised while building the graph for the tasks you asked for. Nothing has run.

### Unknown task

```text
no workspace has a 'tset' task. Did you mean 'test'?
Tasks: build, check, clean, dev, format, lint, test
```

No workspace defines the task. A task listed only in the `tasks` map does not
count: that map configures tasks, it does not create them. The list is every
task any workspace has.

### A dependency on an unknown task

```text
task 'build' depends on 'codegne', but no workspace has a 'codegne' task
Did you mean `codegen`?
Tasks: build, codegen, lint, test
```

The `^` form is checked the same way.

### A workspace sat a task out

```text
web has no 'build' task. Did you mean 'biuld'?
```

A warning: the workspace declares a name one or two edits away. The task was
skipped there and the run carried on.

```text
no 'lint' task in docs, web, so they sit this one out
```

A note, shown on the live display and with `-v` in the stream. Not a problem.

### A persistent task is depended on

```text
task 'dev' in workspace 'app' is persistent, so no other task may depend on it
```

`dev`, `start`, `serve` and `watch` are persistent by default. A one-shot
`start` needs `"persistent": false`.

### The task graph has a cycle

```text
the task graph has a cycle through api:build. Workspace dependencies come from each manifest and from `dependsOn` in `overrides`; one of them points back the way it came
```

Two packages that list each other as dependencies, or two tasks whose
`dependsOn` point at each other.

## Toolchain resolution and provisioning

Raised on the first real run, `lattice setup`, or `lattice env` that needs an
engine, before any task starts. `--dry-run` never checks engines. Inside a run,
each is wrapped:

```text
workspace 'web' cannot run
```

Every engine error has the same shape: a headline, then labeled lines.

```text
engine 'node' does not match the version this repo pins
  wanted   >=22
  found    20.11.1 at /usr/local/bin/node
  ran      node --version
  fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

`output` carries what a command printed, the last 20 lines of it for a long
install.

### Checking a host tool

| Headline | Cause | Fix |
| --- | --- | --- |
| `is not installed, or not on PATH` | The program was not found | Install it, or add an `installCmd` |
| `does not match the version this repo pins` | The host version fails the constraint | Install the pinned version, or add an `installCmd` |
| `could not be checked: its version command failed` | The version command exited non-zero | Fix the tool, or point `versionCmd` at a command that works |
| `could not be checked: Lattice found no version in what it printed` | No digits in the output | Point `versionCmd` at a command that prints the version |
| `has a version constraint but no way to check the installed version` | Not a well-known engine, and no `versionCmd` | Add `versionCmd` |

A `gradle` or `maven` engine without its own `versionCmd` is checked through the
build's wrapper, so `ran` reads `./gradlew --version`.

### Installing a tool

| Headline | Cause |
| --- | --- |
| `could not be installed: its installCmd exited with code <n>` | The install command failed; its output follows |
| `was installed, but its version command failed` | The installed tool is not under `bin`, or does not run |
| `installed a version that does not match its constraint` | The install worked and pinned the wrong version |
| `was installed, but Lattice could not read a version from it` | A constraint cannot be checked against what was installed |
| `has the constraint '<c>' but no way to check what was installed` | A constraint on a tool with no version rule |

A failed install leaves nothing pinned, so the next run retries from scratch.

```text
the pinned toolchain cannot be put on PATH, because a directory in it contains a character PATH cannot hold: /repo:2/.lattice/toolchains/just/1.30.0-1a2b3c4d/bin
```

The repo's path holds `:` on unix, or `"` on Windows. Move the checkout. Lattice
never falls back to the host tool in this case.

```text
failed to create toolchain dir /repo/.lattice/toolchains/just
```

```text
failed to move toolchain into place: <staging> -> <final>
```

Filesystem errors, with the OS error indented under them.

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
failed to cache outputs: outputs ["dist"] matched only empty directories, so nothing was cached. Check that the task writes its files where the patterns point
```

```text
failed to cache outputs: failed to create cache dir /repo/.lattice/cache
```

The second and third are the ones you are most likely to hit. In both, the task
succeeded, declared `outputs`, and produced no file the patterns cover, so
Lattice stores nothing and the next run misses again.

The two messages differ in what the patterns matched. `no files matched outputs`
means they matched nothing at all. `matched only empty directories` means a
pattern named a directory that exists and holds no files. A bare `outputs:
["dist"]` against an empty `dist/` reads that way. Storing an archive of an empty
directory would make every later run a hit that restores an empty `dist/` over
whatever the last real run wrote, so Lattice refuses that case too.

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
failed. Each of those lines carries the exit code the command returned and how
long it ran, and the task's captured output prints under it.
`--sequentially` applies the same rule per phase: a failing phase stops the
remaining phases unless `--continue` is also set, in which case every phase runs
and the process still exits `1`.

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

Both are reported as that task's failure. A task can also fail before its shell
runs at all, when the workspace's pinned toolchain cannot go on the task's
`PATH`. See [Installing a tool](#installing-a-tool).

### A task overran its `timeout`

```text
app:build: timed out after 10m and was stopped
```

A warning for that task, printed just before its `FAILED` line, so it shows even
when the task had already streamed output. The task's whole process group is sent `SIGTERM`, given five seconds, then killed.
The duration is rendered in the largest exact unit, so `"timeout": "600s"`
reports `10m`. The task counts as a failure, so the run then stops or carries on
under `--continue` like any other failure. A persistent task has no effective
timeout whatever `timeout` says.

A task whose command ignores `SIGTERM`, or that backgrounded something still
holding its output pipes, reports up to five seconds after the timeout itself.
Those five seconds are the grace period, and Lattice kills the process group
outright once it runs out. Such a task previously never reported at all. The run
kept waiting on pipes that nothing was going to close.

### The run was interrupted

Ctrl-C, or a `SIGTERM` delivered to Lattice, stops the run. Every running task's
process group is sent `SIGTERM`, given five seconds, then killed. The process
exits `130` rather than `1`:

```text
interrupted. Lattice stopped the tasks that were still running
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
there is no workspace named 'wbe'. Workspaces: api, web
```

A name passed to `lattice setup` that is not a workspace. Fatal, and raised
before Lattice provisions or installs anything. With no workspaces at all the
tail reads `Lattice found no workspaces`.

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

```text
web: dependencies installed, but .lattice/setup/apps%2Fweb.marker could not be written, so the next `lattice setup` will install again: Permission denied (os error 13)
```

The install succeeded and writing the marker that records it failed, so nothing
remembers the install and the next `lattice setup` repeats it. A warning rather
than an error, and the command still succeeds. The path is the workspace's own
marker under `.lattice/setup`, and the error from the operating system follows
the colon. See [`lattice setup`](/lattice/docs/cli#lattice-setup) for how the
marker is named.

An installer that wants to prompt is a third case. `lattice setup` gives the
install command no stdin, so an installer that asks for a password or a
confirmation reads end-of-file and exits non-zero. Its own message about what it
wanted appears under `web: \`pnpm install\` failed`. The install command used to
inherit the terminal and wait on a prompt nothing was showing, until something
killed the run. Supply the credential without a prompt, through an environment
variable or a configured keychain helper. Running the installer once yourself,
outside Lattice, also works.

### Two conditions that are not errors

Both print a message and exit `0`. A `--filter` that matches no workspace:

```text
lattice: no workspaces matched filter '<pattern>'.
```

And a repo with no workspaces at all:

```text
lattice: found no workspaces. The repo root names none in a workspace file and holds no project Lattice recognizes. List workspace directories under `workspaces` in lattice.json, like "apps/*".
```

## Version pinning and self-update

Raised by `lattice upgrade` and by the automatic handover to a pinned version
that runs before most commands. See [Upgrading](/lattice/docs/upgrading) for the
normal path.

### Not a version

```text
'v1.x' is not a version. Write it like 0.2.0
```

The argument to `lattice upgrade` did not parse as semver. Fatal. The indented
line carries the parser's own complaint, such as `unexpected
character 'x' while parsing minor version number`.

```text
lattice.json pins `latticeVersion` as "1.x", which is not a version. Write it like 0.2.0, or run `lattice upgrade <version>` to set it
```

The same check against a repo's `latticeVersion`, run before the handover to a
pinned build. Fatal, and it names the field and the file rather than failing on a
download of a release that cannot exist. The indented lines carry the
message shown above this one, `'1.x' is not a version. Write it like 0.2.0`, with
the parser's complaint under that. Lattice accepts a `v` prefix and strips it, so
`"v1.1.1"` and `"1.1.1"` pin the same release. When that release is the running
build there is no handover to make, and the pin does nothing. A repo whose own
tags carry the `v` prefix used to fail here on a download.

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
tool that left `PATH` mid-run. Fatal, with the OS error on the indented line under it.

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
<url>` detail on the indented line under it. Retry, or name a version.

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
message on the indented line under it instead, such as `unexpected end of file`. The third
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
Run with --no-version-check to use lattice 1.1.1 instead
```

A binary Lattice installed under `.lattice/bin` hands the invocation to the
version the repo pins, installing it first if needed. This wraps whatever
install or download error stopped that, so the indented lines hold one of
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
failed. Fatal, with the OS error on the indented line under it.

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
