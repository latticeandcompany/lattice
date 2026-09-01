---
name: lattice
description: >
  Reference for Lattice, a CLI that runs and caches a monorepo's tasks in any
  language and pins the versions of the tools those tasks use. Everything it
  does is declared in one `lattice.json` at the repo root. Use this skill when:
  (1) the repo contains a `lattice.json`,
  (2) the user asks to build, test, lint, or run anything through `lattice`,
  (3) adding Lattice to a repo, or declaring a new workspace, task, or engine,
  (4) a `lattice` run fails with a driver, engine, cache, config, or graph error,
  (5) a task is missing the cache or hitting it when it should not,
  (6) wiring `lattice` into CI.
license: ISC
compatibility: lattice 1.0.0+
metadata:
  author: latticeandcompany
  version: "1.0.0"
allowed-tools: Bash(lattice:*), Read, Write, Edit, Glob, Grep
---

# Lattice

Lattice is one CLI with two halves, usable together or apart. It runs a
monorepo's tasks in dependency order and in parallel, caching each result by
content so unchanged work is skipped. It also pins the versions of the tools
those tasks use. Everything it knows comes from one `lattice.json` at the repo
root.

Lattice does not replace the commands a repo already has. Each workspace runs its
real command, such as `cargo test`, `pnpm run build`, or `./gradlew check`,
either detected from the directory or written out in `scripts`.

## Vocabulary

Use these words. Using another one for the same thing produces configs that read
as if they came from a different tool.

| Term | Meaning |
| --- | --- |
| repo | The git repository, with `lattice.json` at its root |
| workspace | One directory listed in `workspaces[]`, with a literal `path`. The unit of task running and caching |
| task | A named entry under `tasks`, resolved to a concrete shell command per workspace |
| driver | The tool detected or declared for a workspace that turns a task name into a command |
| engine | A versioned tool constraint under `engines` |
| toolchain | An engine Lattice has installed under `.lattice/toolchains/` |
| cache hit | A task whose stored result was verified and restored instead of running |
| saved time | Task time a run's cache hits skipped, summed from what each entry recorded. Not wall clock |
| persistent task | A task declared `persistent: true`, such as a dev server |

A workspace is not a project, a package, or a project directory. The repo is not
a workspace.

## Orient before you act

1. Find the config. Every command walks up from the current directory to the
   nearest `lattice.json` and treats that directory as the repo root, so
   `lattice` works from any subdirectory. No config in this directory or any
   parent is a hard error.
2. Read `lattice.json` end to end. `workspaces` says what the repo is divided
   into, `tasks` says what verbs exist, `engines` says which tool versions are
   enforced.
3. Ask the binary, not the config, what a task will do:

   ```sh
   lattice run build --dry-run
   ```

   This loads and validates the config, resolves every `workspace:task` node and
   its exact shell command, prints them in dependency order, and exits without
   running or caching anything. It is the cheapest way to confirm both that an
   edit to `lattice.json` parses and that the commands are the ones you meant.

Running a task name that is not a key under `tasks` fails and lists the names
that do exist. `lattice run --help` and `lattice <command> --help` are
authoritative if anything here disagrees with the installed binary.

## What does not exist

Every item here is something models invent. None of it parses.

| Not a thing | What to write instead |
| --- | --- |
| A `projects` key | `workspaces`, an array of objects |
| A `pipeline` key | `tasks` |
| A glob in a workspace `path` (`"apps/*"`) | One entry per directory, each with a literal `path` |
| A `glob` field on a workspace | Nothing. Declare each directory |
| A workspace as a bare string (`"apps/web"`) | `{ "name": "web", "path": "apps/web" }` |
| `workspace#task` in `dependsOn` | `^task` for dependencies, bare `task` for this workspace |
| A glob in `dependsOn` | Nothing. Name tasks literally |
| `env` as `"NAME=value"` pairs | Variable names only: `["DATABASE_URL"]` |
| `extends` or `include` | Nothing. One config file per repo |
| `maxCacheSize` as a number | A string: `"10GB"` |
| A flag on `lattice stats`, such as a json or since flag | Nothing. It takes none |
| Any other key at any level | Nothing. An unknown key fails the load |

## Rules that will bite you

- **A persistent task blocks.** Once a run starts a task declared
  `persistent: true`, `lattice run` streams its output and waits. Never call one
  from a blocking foreground command. Run it in the background, or run only its
  non-persistent prerequisites. Four things end such a run: the first `Ctrl-C`, a
  `SIGTERM`, the exit of every persistent task, or the failure of any task in the
  run. If the process exits anyway, the run reports `EXITED (code <n>)` and ends,
  counting a non-zero exit as a failed task.
- **An interrupted run reports no failures.** `Ctrl-C` and `SIGTERM` stop the
  scheduler and terminate each task's process group. A task stopped that way
  prints no `FAILED` line and is not counted, so read the exit code. `130` is an
  interrupt and `1` is a real failure. Do not infer success from `0 failed`.
- **A manifest-driven workspace can only run a script its manifest declares.**
  `npm`, `pnpm`, `yarn`, and `bun` read `scripts` in `package.json`. `deno` reads
  `tasks` in `deno.json` or `deno.jsonc`. A task the manifest does not declare is
  not run there. The task drops out of the graph and the run carries on. Lattice
  does not fabricate `npm run <task>`. Every other driver takes the task name on
  its command line, so `lattice run test` in a `cargo` workspace runs
  `cargo test` whether or not a target exists. To run a task in a workspace whose
  manifest does not declare it, add a `scripts` entry in `lattice.json`.
- **A `persistent: true` task infers a command only where one can be trusted.**
  A manifest-driven workspace runs it if the manifest declares it, and a task
  runner (`just`, `task`, `turbo`, `nx`, `rake`, `mix`) runs it because those
  run the tasks the repo declared to them — `dev` in a `turbo.json` workspace
  resolves to `turbo run dev`. Every other driver takes the task name as a
  subcommand and there is no `cargo dev`, so a persistent task there needs a
  `scripts` entry.
- **A skipped task warns when it looks like a typo.** A workspace whose manifest
  declares a script map without the requested task prints
  `<ws> declares scripts but no "<task>", so the task was skipped`, plus either a
  `Did you mean` suggestion or advice to declare the task. `tasks` replaces
  `scripts` in the message for deno. The warning fires on `--dry-run` too, covers
  only the tasks being run, and is *not* narrowed by `--filter`. A manifest with
  no script map at all produces no warning, and neither does an `auto: false`
  workspace. Treat the warning as a config problem, not as a run failure.
- **A declaration does not override a higher-ranked driver.** Detection resolves
  by role rank first: task runner, then package manager, then build tool, then
  runtime. An `engines` declaration only breaks a tie among candidates already at
  the top rank. Declaring `pnpm` in a workspace that has a `turbo.json` still
  resolves to `turbo`. Use `"auto": false` and `scripts` to force a command.
- **`--filter` selects the roots of a run, not all of it.** It matches
  workspaces whose `name` *contains* the pattern, a substring match on `name` and
  never on `path`. The graph then adds everything those workspaces depend on,
  transitively, tagged `(dependency)` under `--dry-run`. Nothing that depends *on*
  a match is included.
- **A task with no `inputs` hashes its whole workspace.** That walk honors
  `.gitignore` in the workspace and every ancestor directory. It is correct but
  slower than it needs to be, and it re-runs on changes the command never reads.
- **Declared `inputs` ignore `.gitignore`.** Once `inputs` is present, only
  `.lattice`, `.git`, `.hg`, `.svn`, and `.jj` are skipped. A glob that reaches
  into `node_modules` or `target` will hash it. Symlinks are not skipped. The
  walk descends a symlinked directory, and a symlinked file contributes the path
  it points at rather than the bytes on the other end.
- **A key written twice in `tasks`, `engines`, or `scripts` fails the load.**
  Editing a config by appending is how this happens. The message names the key,
  the container, and the position, and its second line reads `Keep one of them:
  the second replaces the first, so only the last would take effect`. Merge the
  two entries by hand. There is no last-wins behavior to rely on.
- **Every `scripts` key must name a task declared under root `tasks`.** A
  `scripts` entry supplies the command for the root task of the same name and
  nothing else, so a key outside `tasks` can never run. Such a key is a load
  error with a `Did you mean` suggestion, not a no-op.
- **`inputs` cannot name a file above the workspace.** Patterns are relative to
  the workspace directory, and `tasks` is shared by every workspace, so a shared
  root file has no `inputs` spelling that means the same thing everywhere. Put it
  in the root-level `globalDependencies`, and repo-wide variables in `globalEnv`.
  Leave a shared root file out of both and every task hits cache with an artifact
  built before it changed.
- **A cache hit deletes what `outputs` matched before restoring.** Anything
  written by hand into a directory an `outputs` glob covers is gone on the next
  hit.
- **A `dependsOn` that names nothing is an error, not a no-op.** A workspace
  `dependsOn` must name a declared workspace, and a task `dependsOn` must name a
  defined task. Either miss is rejected at load, with the nearest name offered.
  Do not "fix" one by deleting the reference; the ordering it was written for is
  usually real.
- **An unknown key in `lattice.json` fails the load.** Every command that reads
  the config rejects a key Lattice does not define, naming the key, the object it
  sits in, its line and column, and the nearest valid field. Keep
  `"$schema": ".lattice/schema.json"` so the editor flags the same thing while
  you type.
- **`lattice run` installs nothing.** It expects dependencies and toolchains to
  be in place. `lattice setup` provisions `engines` and runs each workspace's
  native installer. `setup` skips a workspace whose install marker under
  `.lattice/setup` is newer than every lockfile governing that workspace. Those
  lockfiles are the one in the workspace *and every one above it* up to the repo
  root, so a hoisted npm, pnpm, or yarn tree with one root lockfile works.
  `setup` refuses a workspace name the config does not declare. It gives the
  installer no stdin, so an installer that prompts fails immediately instead of
  hanging.
- **`lattice` never invents a driver.** When a workspace's evidence is ambiguous
  or absent, the run halts with a fix printed. Do not work around a halt by
  guessing a command into `scripts` before asking. See
  [When detection halts](#when-detection-halts).
- **An npm-installed binary ignores `latticeVersion`.** Lattice also ships as
  `@latticeandcompany/lattice` on npm, a wrapper whose six platform packages each
  carry one prebuilt binary. Only a binary under `.lattice/bin` switches to the
  pinned version. One in `node_modules`, one from `cargo install`, and one built
  from source all run as they are. The wrapper prints a warning to stderr when the
  pin and the installed version disagree, then runs anyway, so a repo can pin one
  version and run another. If a run behaves like a different release, compare
  `lattice version` with `latticeVersion` before anything else, and bring the two
  into line rather than silencing the warning.

## Running tasks

```sh
lattice run build                      # one task, across every workspace that has it
lattice run lint test build            # merged into one graph; shared deps run once
lattice run lint test build -s         # one graph per task, each to completion
lattice run test --filter api          # names containing api, plus what they depend on
lattice run build --dry-run            # resolve and print, run nothing
lattice run lint test --continue       # keep going past a failure, still exit 1
lattice run build --concurrency 4      # cap parallelism; default is the CPU count
lattice run build --no-cache           # neither read nor write the cache this run
lattice run build --force              # re-run and overwrite the stored entry
lattice run build -v                   # raw workspace:task: lines, not the live display
```

`-v` (`--verbose`) is worth passing whenever you need to read or grep the
output. It prints each task's hash and cache outcome as plain lines, and it is
the only place those lines appear. The live display prints no per-task trace.
Output is already in raw mode when stdout is not a terminal or `CI` is set to any
value, so a command run programmatically usually gets it for free.

A failed task's line reads `web:build: FAILED (code 1) after 1.02s`, and the
task's captured output follows it, in the order the task produced it across both
streams. The line drops the detail it does not have. A task a signal killed, and
one stopped for overrunning its `timeout`, have no exit code, so the line reads
`FAILED after 30.00s`. A task that failed before its command started has
neither, so the line reads `FAILED`. Its cache key would not compute, or its
shell would not spawn.

`--concurrency 0` is accepted and means the default. `--no-cache` neither reads
nor writes; `--force` skips the read and still writes, which is what replaces a
bad entry.

Exit codes:

| Code | Meaning |
| --- | --- |
| `0` | Success. Also a filter that matched nothing, and a repo with an empty `workspaces` array |
| `1` | Any error Lattice raises, including a failed task |
| `2` | The command line itself was rejected |
| `130` | Ctrl-C or `SIGTERM`. Running tasks were killed on the way out |

## Editing `lattice.json`

Eight top-level keys, all optional. `{}` is a valid config.

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0",
  "workspaces": [
    { "name": "core", "path": "libs/core" },
    { "name": "api", "path": "services/api", "dependsOn": ["core"] },
    {
      "name": "docs",
      "path": "docs",
      "auto": false,
      "scripts": { "build": "python3 -m mkdocs build" }
    }
  ],
  "engines": { "node": ">=20.0.0" },
  "globalDependencies": ["tsconfig.base.json", "proto/**"],
  "globalEnv": ["NODE_ENV"],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "package.json"],
      "outputs": ["dist/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["src/**/*", "tests/**/*"],
      "timeout": "10m"
    },
    "dev": { "persistent": true }
  },
  "settings": { "maxCacheSize": "10GB" }
}
```

A workspace needs only `name` and `path`. `auto` defaults to `true`, which means
the driver and each task's command are read from the directory's own evidence.
`scripts` maps a task name to an exact shell command and always wins over
anything inferred, whether or not `auto` is true, so you can override one task
and leave the rest detected. With `auto: false` nothing is inferred and `scripts`
is the only source of commands; a task requested against such a workspace with no
matching entry fails rather than being skipped.

In a task's `dependsOn`, `^build` means "`build` in each workspace this one
depends on", following that workspace's own `dependsOn`. A bare `build` means
"`build` in this same workspace". Those two tokens are the whole vocabulary. A
workspace's `dependsOn` changes no ordering on its own; it takes effect only
where some task uses `^`.

Every field, type, default, and validation error:
`references/configuration.md`.

## Caching decisions

The cache key is a sha256 over ten separately hashed components:

| Component | What goes in |
| --- | --- |
| `environment` | Lattice version, OS, architecture, shell, workspace name, task name |
| `command` | The resolved shell command |
| `toolchain` | The resolved toolchain identity string |
| `dependencies` | The cache key of every task this one depends on |
| `patterns` | The `inputs`, `outputs`, and `ignore` glob lists themselves |
| `env` | The names in the task's `env`, with their resolved values |
| `globalEnv` | The names in the root `globalEnv`, with their resolved values |
| `inputs` | Every file `inputs` matched, minus `ignore` and minus `outputs`: its path, its executable bit, and its contents. A symlink contributes its target path instead of contents. With no `inputs`, the whole workspace |
| `manifests` | Contents of every recognized manifest and lockfile in the workspace, plus lockfiles at the repo root |
| `globalDependencies` | The root `globalDependencies` patterns and the contents of what they match |

Nothing else is consulted. `timeout`, `cache`, and `persistent` are not part of
the key, and neither are the project's dependency bin directories Lattice puts
on a task's `PATH`. What the key tracks about tools is the resolved toolchain
identity, not where a binary was found.

Two consequences worth knowing before debugging a hit or a miss:

- Editing a dependency re-runs its dependents. A workspace is one node, so a
  dependent re-runs whenever its dependency's key moves, even if the particular
  files it reads did not change.
- A task's own `outputs` never affect its key, so there is no need to repeat them
  in `ignore`.
- `chmod +x` on a hashed file moves the key. Lattice hashes the executable bit
  with every file's contents, because the artifact restores file modes. Nothing
  else about the mode counts.
- Re-pointing a symlink moves the key, even when the new target holds identical
  bytes. Lattice hashes the link's target path, not the bytes it points at.

That makes six fields yours to get right:

- `inputs` is what the command reads. Omit it and the whole workspace is hashed,
  which is safe but slow. Declare it to narrow the key: under-declare and you get
  a stale hit, over-declare and unrelated edits force rebuilds. Start broad and
  tighten.
- `ignore` subtracts from what `inputs` matched. Reach for it when a broad glob
  sweeps in logs or an inner tool's cache directory.
- `outputs` is what gets archived on success and restored on a hit. A file the
  command produces that no `outputs` glob matches is never saved. A task that
  declares `outputs` and produces no file under them is not cached at all, and
  warns. The warning reads `no files matched outputs [...]` when the patterns
  matched nothing, and `outputs [...] matched only empty directories` when a bare
  pattern like `"dist"` matched a directory holding nothing. `test` and `lint`
  usually need none. A hit clears what the recorded `outputs` match, directories
  included, so a directory the cached run never produced does not survive a hit.
- `env` holds variable *names*, not `NAME=value` pairs. The resolved value is
  hashed, and a set one is also exported to the task. An unset name is still
  hashed as declared, so adding a name moves the key. Lattice does not read
  `.env` files.
- `globalDependencies` (root level) holds repo-root-relative globs hashed into
  every task's key. It is the only way to cover a file above a workspace: a base
  `tsconfig.json`, a shared schema directory, a root `.env`. Editing anything it
  matches makes every task miss, so list only what genuinely crosses workspaces.
- `globalEnv` (root level) is `env` for the whole repo. Same resolution, hashed
  into every key, and not re-exported to the task, which already inherits it.

`lattice init` writes both when a selected workspace's `turbo.json` declares
them. Prune what it brought over on the same rule: a global dependency that does
not really cross workspaces costs every task a miss.

Set `cache: false` to opt one task out entirely. `persistent: true` is never
cached regardless. The cache lives in `.lattice/cache` and is safe to delete at
any time. `settings.cacheDir` moves it. The value has to name a directory inside
the repo, and it cannot be the repo root, because `lattice prune` deletes
archives and partial writes in whatever directory the value names. With
`settings.maxCacheSize` set, every run that writes evicts least-recently-used
entries down to it. With no budget set, the cache grows without limit and
`lattice prune --max-size <size>` sweeps it by hand.

Files left behind by an interrupted run are reclaimed only once they are an hour
old, so a `prune` immediately after a `SIGKILL` reports `removed 0 artifacts`.
The delay lets two `lattice` processes share one cache directory without deleting
each other's writes.

**Debugging a miss:** run with `-v`. Each miss names the component that moved:

```text
lattice: web:build: hash a1b2c3d4e5f6a7b8
lattice: web:build: cache miss: inputs changed
```

Two misses name no component: `cache miss (nothing cached for this task yet)`
and `cache miss (the entry for this key is no longer in the cache)`. Start with
whatever the line names rather than bisecting the config.

**What the cache saved:** any run with hits ends its summary with the task time
those hits skipped — `lattice: 10 tasks, 8 cached, 0 failed, 4.20s, 2m 51s
saved` — and the tail is omitted when the figure is zero. Each hit contributes
the duration its entry recorded when it was written, so the figure is task time
and not wall clock: four cached one-minute tasks that would have run in parallel
read as `4m 00s saved`. Do not tell anyone it is time off their wall clock.

`lattice stats` totals those figures across every run the repo recorded. The
record is `stats.jsonl` inside the cache directory, one appended JSON line per
run. A run appends only when it could store and scheduled at least one task, so
`--no-cache` records nothing and `--force` does. It is per-machine, never
committed, untouched by `prune`, and cleared with the cache.
`references/cli.md` has the fields and the full output.

## Pinning tool versions

An `engines` entry's *shape*, and nothing else, selects one of three behaviors.
The tool's name has no effect on which.

| You write | What happens |
| --- | --- |
| Nothing, `{}`, or an object with only `versionCmd` or only `bin` | Uses whatever is on `PATH`. Installs nothing, checks nothing, not even that the tool exists |
| A version constraint and no `installCmd` | Runs the tool's version command on the host and fails before any task starts if it does not satisfy the constraint. Installs nothing |
| An `installCmd` | Installs into `.lattice/toolchains/<engine>/<version>-<hash>/`, version-checks it, pins it, and prepends its `bin` to that task's `PATH` |

`bin` defaults to `"bin"` and is resolved against the install directory. The
value has to be relative, non-empty, and inside that directory. `"/usr/bin"` is
a load error, not a way to point a declared toolchain at a host path.

A version Lattice cannot read is never invented. A provisioned engine with no
`versionCmd` and no built-in rule installs under `unknown-<hash>` and contributes
`<name>=unknown@<hash>` to the cache key. A `version` constraint on such an
engine is an error, not a constraint nobody checks. Older releases recorded
`0.0.0` instead, a version nothing had installed, and fed it into every cache
key.

The bare string form (`"node": ">=20.0.0"`) works only for the 40 engines
Lattice has a built-in version rule for. Any other name needs the object form
with an explicit `versionCmd`, or the config is rejected on load. Root `engines`
are defaults; a workspace's own `engines` win per key.

Nothing about a provisioned engine touches the shell, a profile, or a global
install. The `PATH` change lives in the one child process Lattice spawns for that
task, and `rm -rf .lattice/toolchains` is a complete uninstall.

A task's `PATH` gets one more thing: the project's own dependency bin
directories (`node_modules/.bin`, `vendor/bin`, `.venv/bin`, and the rest),
walked from the workspace directory up to the repo root and added after the
toolchain directories. A `scripts` entry can therefore name a tool the project
installed — `eslint src`, `pytest -q` — with no `npx` or `poetry run` prefix
to find it. The full list is in `references/cli.md`.

The well-known engine list, the driver table, and the `installCmd` contract are
in `references/toolchains.md`.

## When detection halts

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

Lattice gathers candidate tools from three kinds of evidence: an `engines`
declaration, a file the developer authored to pin a tool (`packageManager`,
`.tool-versions`, `mise.toml`, `.nvmrc`, `rust-toolchain.toml`, `gradlew`,
`mvnw`, and others), and a lockfile only one tool produces. It then resolves them
by role rank. A bare `package.json`, `pom.xml`, or `pyproject.toml` identifies an
ecosystem, not a tool, and never produces a candidate at all; such names appear
only in the error's candidate list.

The halt has three causes: two tools of the same role with no declaration to
break the tie, only runtime-level candidates (there is no universal `node
build`), or nothing recognizable in the directory.

Two correct fixes, and the choice is which matches reality:

- Add the suggested `engines` entry naming the tool the workspace actually uses.
  Replace the `>=0.0.0` placeholder with a real constraint. This works only when
  the tool shares the top role rank; see `references/toolchains.md`.
- Set `"auto": false` and write `scripts`. Correct when there is no single right
  answer to infer: the real build step is a wrapper script, or the workspace is
  itself a repo with its own task runner underneath.

Ask the user which tool the workspace actually uses if the evidence does not say.
Do not pick one from the candidate list to make the error go away.

## Adding Lattice to a repo

Order matters. Skipping ahead is what makes an adoption feel like a rewrite.

1. `lattice init` (add `-y` to take its proposal without confirming). It walks
   up to five levels deep for directories holding a recognized manifest,
   skipping hidden, gitignored, dependency, and output directories. It also reads
   the tool versions the repo root already pins in `.tool-versions`, `.nvmrc`,
   `rust-toolchain.toml` or `rust-toolchain`, `.python-version`, `.ruby-version`,
   `.java-version`, `package.json` (`packageManager` and `engines`), and a
   `toolchain` line in `go.mod`. For each workspace it keeps, it reads the task
   list out of the file that workspace's driver reads — `turbo.json`, `nx.json`,
   `package.json` scripts, `deno.json`, `composer.json`, a `justfile`, a
   `Taskfile.yml`, `pyproject.toml`, `Pipfile` — and writes every task it finds.
   Then it writes
   `lattice.json`, a committed `.lattice/schema.json`, and five `.gitignore`
   lines (`.lattice/cache/`, `.lattice/toolchains/`, `.lattice/bin/`,
   `.lattice/setup/`, and `.lattice-setup-marker`). Without a TTY it behaves as
   `-y`, and writes a bare skeleton if the scan finds nothing.
   It refuses to overwrite an existing `lattice.json` without `--force`.
2. Check what it proposed. Run `lattice run <task> --dry-run` on a task it wrote
   and confirm each resolved command is the one that directory already runs by
   hand; delete any workspace that is not one. A workspace whose driver takes
   the task name on its command line — `cargo`, `go`, `gradle` and the rest —
   publishes no list to read, so it contributes only `build`; add its other
   tasks by hand. `init` leaves out directories whose driver stayed ambiguous,
   and names them; bring one in by declaring its driver in `engines`, then
   adding the workspace.
3. Review the `inputs` and `outputs`. A `turbo.json` brings its own over, along
   with each task's `dependsOn`, `env`, `persistent`, and `cache`, and the
   repo's `globalDependencies` and `globalEnv`. Everything else arrives as an
   empty task, so declare them there.
4. Keep only the `engines` whose guarantee is actually needed. `init` proposes
   every pin it found, which is usually more than a repo needs enforced.
5. Leave the install markers alone. `lattice setup` writes them under
   `.lattice/setup`, `init` covers that directory in `.gitignore`, and nothing
   under `.lattice` is ever hashed into a cache key. Older releases wrote a
   `.lattice-setup-marker` inside each workspace, which did reach the key of a
   task with no `inputs`. `init` still ignores that name, and a successful
   install deletes the file.

Declaring a workspace stops nothing. Its existing `package.json` scripts,
`Makefile`, and CI steps keep working. Bring workspaces in one at a time and use
`--filter` to scope a run while migrating.

A subtree that already has its own task runner does not need flattening. Declare
it as one workspace with `auto: false` whose `scripts` call that runner, put the
inner runner's own cache directory in `ignore` (its per-invocation state would
otherwise move the key on every run), and Lattice schedules and caches it as a
single node. Flatten it instead when another workspace needs to depend on one
specific inner package, or when you want a rebuild narrowed below the whole
subtree.

## In CI

`lattice setup` as its own step, then `lattice run`. Restore and save the cache
directory (`.lattice/cache`, or `settings.cacheDir`) with a rolling key, and set
`settings.maxCacheSize` so every run keeps it bounded. Use
`lattice prune --max-size <size>` before saving to apply a different limit in CI
than locally. `--continue` is the right shape for a CI run: it reports every
failure in one pass and still exits `1`. `CI` being set already forces plain
output, so `-v` is redundant there. The ledger `lattice stats` reads lives in the
cache directory, so restoring the cache restores the run history with it.

Give tasks that can hang a `timeout`, so a stuck run fails instead of burning the
job's whole budget and saving no cache. A cancelled job sends `SIGTERM`: Lattice
stops every running task's process group and exits `130`, which is worth
distinguishing from `1` if the pipeline branches on the exit code.

## Verify your work

- `lattice run <tasks> --dry-run` confirms the config parses and the commands are
  right. It does not validate engines; only a real run or `lattice setup` does.
- `lattice run <tasks> -v` is a real run, with each task's hash and cache outcome
  on its own line.
- Run it a second time. Every task should report a cache hit, and the run should
  end with `lattice: full power, nothing to run`. That line prints only when
  nothing executed, and it was `full cache` in earlier releases. If a task
  misses twice in a row with nothing changed, the miss line names the component
  responsible; start there.
- `lattice version` says which binary actually ran, which matters when the repo
  pins `latticeVersion`.

## References

- `references/configuration.md` covers every `lattice.json` field, type,
  default, and validation error.
- `references/cli.md` covers every command, flag, exit code, environment
  variable, and the precedence between them.
- `references/toolchains.md` covers the driver table, the well-known engine
  list, driver resolution, and provisioning mechanics.
- `references/troubleshooting.md` maps symptom to cause to fix.

Full documentation: <https://latticeandcompany.github.io/lattice/docs>
