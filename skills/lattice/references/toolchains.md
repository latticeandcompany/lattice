# Drivers, engines, and provisioning

A **driver** is the tool that turns a workspace's named task into a shell
command. An **engine** is a versioned tool named under `engines`. The two are
resolved independently. Every driver is also a well-known engine name, but not
every engine is a driver.

## How a driver is chosen

Lattice collects candidate tools for a workspace from three kinds of evidence,
then resolves them by **role rank first** and evidence second.

### Step 1: collect candidates

| Evidence | What counts |
| --- | --- |
| Declaration | Any driver name that is a key in this workspace's merged `engines` map (root `engines` plus the workspace's own) |
| Native file | `.nvmrc`, `packageManager` in `package.json`, a tool name in `.tool-versions`, a tool name under `[tools]` in `mise.toml` or `.mise.toml`, `rust-toolchain.toml`, `rust-toolchain`, `.python-version`, `.ruby-version`, a `ruby ` directive in `Gemfile`, `.java-version`, a `toolchain ` directive in `go.mod`, `gradlew`, `mvnw`, `deno.json`, `deno.jsonc` |
| Lockfile | Any other fingerprint file in the driver table below |

One tool can be named by several kinds of evidence. Lattice keeps the strongest
per tool: declaration, then native file, then lockfile.

### Step 2: resolve by role

Every driver declares one or more roles. A driver competes with its
highest-ranked role.

| Role | Rank | Drivers with this as their highest role |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift`, `stack`, `cabal` |
| Package manager | 2 | `pnpm`, `yarn`, `npm`, `bun`, `uv`, `poetry`, `pdm`, `pipenv`, `pip`, `bundler`, `nuget`, `pod`, `composer`, `dart` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake`, `mix` |

The resolution rules, in order:

1. Take the highest role rank among all candidates.
2. If that rank is Runtime (0), halt. A runtime cannot drive a named task.
   There is no universal `node build`.
3. Keep only the candidates at that rank. If exactly one remains, it drives.
4. If several remain, and exactly one of them is a declaration, that one drives.
5. Otherwise, halt with an ambiguity error.

**Role rank is checked before evidence rank.** A declaration does not override a
higher-ranked tool. Declaring `"engines": { "pnpm": ">=8" }` in a workspace that
also has a `turbo.json` resolves to `turbo`, because a task runner outranks a
package manager. To force `pnpm` there, set `"auto": false` and write `scripts`.

Consequences worth memorizing:

- `.nvmrc` plus `pnpm-lock.yaml` resolves to `pnpm`. Different roles compose;
  `node` stays in the engine map and can still be version-checked or
  provisioned.
- `pnpm-lock.yaml` plus `bun.lockb` halts. Two package managers, same rank, no
  declaration.
- `bun.lockb` plus `.nvmrc` resolves to `bun`. Bun's highest role is package
  manager, which outranks node's runtime.
- `turbo.json` plus `pnpm-lock.yaml` resolves to `turbo`.
- `Gemfile` with a `ruby` directive plus `Rakefile` resolves to `rake`.

### A generic marker is never evidence

A file that names a language but not a tool never produces a candidate. These
markers are read only to build the candidate list an ambiguity error suggests:

| Marker present | Tools the error names |
| --- | --- |
| `package.json` | `pnpm`, `npm`, `yarn`, `bun` |
| `Cargo.toml` | `cargo` |
| `go.mod` | `go` |
| `pyproject.toml` | `uv`, `poetry`, `pdm`, `pipenv` |
| `requirements.txt` | `pip`, `uv`, `poetry` |
| `setup.py` | `pip`, `uv`, `poetry` |
| `Gemfile` | `bundler`, `rake` |
| `pom.xml` | `maven` |
| `build.gradle`, `build.gradle.kts` | `gradle` |
| `composer.json` | `composer` |
| `mix.exs` | `mix` |
| `pubspec.yaml` | `dart` |
| `Package.swift` | `swift` |
| `stack.yaml` | `stack` |
| `cabal.project` | `cabal` |
| a `.sln`, `.csproj`, `.fsproj`, or `.vbproj` file | `dotnet`, `nuget` |

The first marker in that order wins. A workspace holding only a marker halts:

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Candidate drivers: pnpm, npm, yarn, bun
Declare the driver in lattice.json, under this workspace:
  "engines": { "pnpm": ">=0.0.0" }
```

A workspace holding no recognized file at all, or only runtime-level evidence,
halts with the other form:

```text
Error: workspace 'app' has an ambiguous or undeclared driver.
Lattice detected no driver. The directory holds no lockfile, no wrapper, and no native declaration.
Declare the driver in lattice.json, under this workspace:
  "auto": false, "scripts": { "build": "<command>" }
```

The suggested fix names the first candidate that could actually drive tasks. It
falls back to the `auto: false` form when no candidate can.

### `auto: false` disables the consequences, not the detection

With `auto: false` Lattice still runs detection, but an ambiguous or absent
result is never an error and no command is ever inferred. Only `scripts`
supplies commands, and a task requested against such a workspace with no
matching `scripts` entry is fatal rather than skipped.

## The built-in driver table

34 drivers. `{task}` is replaced by the task name. `Language` is the ecosystem
slug Lattice tags the workspace with; 13 slugs are in use, and `just` and `task`
carry none because they sit above whatever the workspace is.

| Tool | Language | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- | --- |
| `node` | node | Runtime | `.nvmrc` | `node --version` | `node {task}` |
| `deno` | node | Runtime, Package manager, Task runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` | `deno task {task}` |
| `bun` | node | Runtime, Package manager | `bun.lockb`, `bun.lock` | `bun --version` | `bun run {task}` |
| `pnpm` | node | Package manager | `pnpm-lock.yaml` | `pnpm --version` | `pnpm run {task}` |
| `yarn` | node | Package manager | `yarn.lock` | `yarn --version` | `yarn {task}` |
| `npm` | node | Package manager | `package-lock.json`, `npm-shrinkwrap.json` | `npm --version` | `npm run {task}` |
| `turbo` | node | Task runner | `turbo.json` | `turbo --version` | `turbo run {task}` |
| `nx` | node | Task runner | `nx.json` | `nx --version` | `nx run {task}` |
| `cargo` | rust | Build tool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo --version` | `cargo {task}` |
| `go` | go | Build tool | `go.sum` | `go version` | `go {task}` |
| `uv` | python | Package manager | `uv.lock` | `uv --version` | `uv run {task}` |
| `poetry` | python | Package manager | `poetry.lock` | `poetry --version` | `poetry run {task}` |
| `pdm` | python | Package manager | `pdm.lock` | `pdm --version` | `pdm run {task}` |
| `pipenv` | python | Package manager | `Pipfile.lock` | `pipenv --version` | `pipenv run {task}` |
| `pip` | python | Package manager | none | `pip --version` | `pip {task}` |
| `python` | python | Runtime | `.python-version` | `python --version` | `python -m {task}` |
| `bundler` | ruby | Package manager | `Gemfile.lock` | `bundle --version` | `bundle exec {task}` |
| `rake` | ruby | Task runner | `Rakefile` | `rake --version` | `rake {task}` |
| `ruby` | ruby | Runtime | `.ruby-version` | `ruby --version` | `ruby {task}` |
| `gradle` | java | Build tool | `gradlew` | `gradle --version` | `./gradlew {task}` |
| `maven` | java | Build tool | `mvnw` | `mvn --version` | `./mvnw {task}` |
| `java` | java | Runtime | `.java-version` | `java -version` | `java {task}` |
| `kotlin` | kotlin | Runtime | none | `kotlinc -version` | `kotlin {task}` |
| `dotnet` | dotnet | Build tool | `global.json` | `dotnet --version` | `dotnet {task}` |
| `nuget` | dotnet | Package manager | `packages.config` | `nuget help` | `nuget {task}` |
| `pod` | swift | Package manager | `Podfile`, `Podfile.lock` | `pod --version` | `pod {task}` |
| `swift` | swift | Build tool | `Package.resolved` | `swift --version` | `swift {task}` |
| `composer` | php | Package manager | `composer.lock` | `composer --version` | `composer {task}` |
| `mix` | elixir | Package manager, Task runner | `mix.lock` | `mix --version` | `mix {task}` |
| `dart` | dart | Package manager | `pubspec.lock` | `dart --version` | `dart pub {task}` |
| `stack` | haskell | Build tool | `stack.yaml.lock` | `stack --version` | `stack {task}` |
| `cabal` | haskell | Build tool | `cabal.project.freeze` | `cabal --version` | `cabal {task}` |
| `just` | none | Task runner | `justfile`, `.justfile` | `just --version` | `just {task}` |
| `task` | none | Task runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` | `task {task}` |

Two drivers have no fingerprint and can only be selected by declaration.
`pip` has none because a `requirements.txt` is read by pip, uv, and pip-tools
alike. `kotlin` has none because a Kotlin project is driven by gradle or maven,
and only a `.tool-versions` entry names the Kotlin toolchain itself.

`nuget` fingerprints `packages.config`, the legacy layout, and not
`packages.lock.json`. An SDK-style project can carry a `packages.lock.json` and
still be a `dotnet` workspace.

## How a task's command resolves

An entry in the workspace's `scripts` map always wins, whether `auto` is true or
false. Only a task with no `scripts` entry falls back to the driver's invoke
template. Two limits on inference:

- For `npm`, `pnpm`, `yarn`, and `bun`, the task name must be a key in
  `package.json`'s `scripts` object. For `deno`, it must be a key under `tasks`
  in `deno.json` or `deno.jsonc`. Lattice never invents a script the manifest
  does not have.
- For every other driver, a `persistent: true` task is never inferred. There is
  no `cargo dev`. A persistent task on a direct-invoke driver needs an explicit
  `scripts` entry.

A task with no command in a workspace is skipped when `auto` is true, and fatal
when `auto` is false. The skipped task drops out of the graph and the run carries
on. An older release fabricated `npm run <task>` for a manifest-driven workspace
that declared no such script, and the fabricated command failed the build.

The skip is not always silent. A warning names a workspace whose manifest
declares a script map without the requested task, because a typo in the script
name looks exactly the same:

```text
warn web declares scripts but no "build", so the task was skipped. Did you mean "biuld"?
```

```text
warn web declares scripts but no "build", so the task was skipped. Declare it in the workspace's manifest, or under "scripts" in lattice.json, if the task should run there
```

```text
warn some tasks were skipped: docs declares scripts but no "build"; web declares scripts but no "build" (did you mean "biuld"?). Declare each in the workspace's manifest, or under "scripts" in lattice.json, if the task should run there
```

`scripts` in that text is the manifest's own word for the map, so deno says
`tasks`. Two or more skips collapse into the third form, in the order
`lattice.json` declares the workspaces. The warning is emitted once per run, on
`--dry-run` as well, and it covers only the tasks being run. `--filter` does not
narrow it, because the warning is about the config rather than about this run's
selection.

Two cases stay silent. A manifest with no script map at all is a complete config
for a package with nothing to build, and a workspace with `auto: false` never
warns.

A manifest Lattice cannot read gets one warning per workspace instead, and still
runs nothing there:

```text
warn web: package.json could not be parsed: expected `,` or `}` at line 4 column 3, so every task it would have named was skipped
```

The other two reasons are `<file> could not be read: <io error>` and
`<file> has a "<key>" that is not an object`. Lattice strips `//` and `/* */`
comments from `deno.json` and `deno.jsonc`, so a commented Deno config resolves
its tasks. `package.json` is parsed as strict JSON, the way npm parses it.

## What `lattice setup` installs per driver

`lattice setup` provisions engines, then runs one dependency-install command per
workspace, chosen by the resolved driver.

| Driver | Install command |
| --- | --- |
| `pnpm` | `pnpm install` |
| `yarn` | `yarn install` |
| `npm` | `npm install` |
| `bun` | `bun install` |
| `deno` | `deno cache .` |
| `cargo` | `cargo fetch` |
| `go` | `go mod download` |
| `poetry` | `poetry install` |
| `uv` | `uv sync` |
| `pdm` | `pdm install` |
| `pipenv` | `pipenv install` |
| `pip` | `pip install -r requirements.txt` |
| `bundler` | `bundle install` |
| `dotnet` | `dotnet restore` |
| `nuget` | `nuget restore` |
| `pod` | `pod install` |
| `swift` | `swift package resolve` |
| `composer` | `composer install` |
| `mix` | `mix deps.get` |
| `dart` | `dart pub get` |
| `stack` | `stack build --only-dependencies` |
| `cabal` | `cabal build --only-download` |
| `gradle` | `./gradlew dependencies`, or `gradle dependencies` with no wrapper |
| `maven` | `./mvnw dependency:resolve`, or `mvn dependency:resolve` with no wrapper |

`node`, `python`, `ruby`, `java`, `kotlin`, `rake`, `just`, `task`, `turbo`, and
`nx` have no install command. Their workspaces get toolchains and nothing else.

After a successful install, `setup` writes an empty marker file under
`.lattice/setup`, one per workspace, named for the workspace's path relative to
the repo root with `/` written `%2F` and a literal `%` written `%25`. So
`apps/web` gets `.lattice/setup/apps%2Fweb.marker`, `apps-web` gets
`.lattice/setup/apps-web.marker`, and the repo root as a workspace gets
`.lattice/setup/.marker`. Encoding the separator rather than replacing it means
two paths can never share a marker.

`setup` compares lockfiles against that marker's mtime to decide whether to
reinstall. The lockfiles it compares are the ones in the workspace and in every
directory above it up to the repo root, so the one lockfile a hoisted npm, pnpm,
or yarn tree keeps at the root governs every workspace under it. `--force`
reinstalls regardless.

`lattice init` writes `.lattice/setup/` to `.gitignore`, and nothing under
`.lattice` is ever walked for a cache key, so the marker cannot move a hash.
Older releases wrote a `.lattice-setup-marker` inside each workspace directory,
which did reach the key of a task with no `inputs`. One of those still counts as
a marker, so upgrading mid-project reinstalls nothing. `init` still ignores that
name, and a successful install deletes the file.

A marker that cannot be written is a warning, and the install runs again next
time:

```text
lattice: warning: web: dependencies installed, but .lattice/setup/apps%2Fweb.marker could not be written, so the next `lattice setup` will install again: Permission denied (os error 13)
```

## Well-known engines

An `engines` value may be a bare constraint string only for a name in this
table. Any other name must use the object form with an explicit `versionCmd`, or
the config is rejected when it loads. 40 names:

| Engine | Version rule |
| --- | --- |
| `node` | `node --version` |
| `deno` | `deno --version` |
| `bun` | `bun --version` |
| `pnpm` | `pnpm --version` |
| `yarn` | `yarn --version` |
| `npm` | `npm --version` |
| `rust` | `rustc --version` |
| `cargo` | `cargo --version` |
| `go` | `go version` |
| `python` | `python --version` |
| `python3` | `python3 --version` |
| `pip` | `pip --version` |
| `uv` | `uv --version` |
| `poetry` | `poetry --version` |
| `pdm` | `pdm --version` |
| `pipenv` | `pipenv --version` |
| `ruby` | `ruby --version` |
| `bundler` | `bundle --version` |
| `rake` | `rake --version` |
| `java` | `java -version` |
| `kotlin` | `kotlinc -version` |
| `gradle` | `gradle --version` |
| `maven` | `mvn --version` |
| `dotnet` | `dotnet --version` |
| `nuget` | `nuget help` |
| `swift` | `swift --version` |
| `pod` | `pod --version` |
| `php` | `php --version` |
| `composer` | `composer --version` |
| `elixir` | `elixir --version` |
| `mix` | `mix --version` |
| `dart` | `dart --version` |
| `haskell` | `ghc --version` |
| `ghc` | `ghc --version` |
| `stack` | `stack --version` |
| `cabal` | `cabal --version` |
| `just` | `just --version` |
| `task` | `task --version` |
| `turbo` | `turbo --version` |
| `nx` | `nx --version` |

Six names are engines but not drivers: `rust`, `python3`, `php`, `elixir`,
`haskell`, and `ghc`. They pin a compiler or interpreter that some other tool
drives tasks with. `nuget help` is the version rule because `nuget.exe` has no
`--version` flag and prints `NuGet Version: x.y.z` first.

Version parsing takes the first run of digits and dots after the first digit in
the command's output, then reads it as major, minor, patch with missing parts
set to zero. `v20.11.1`, `go1.22`, `rustc 1.75.0 (abc 2024)`, and `1.75` all
parse. Banner text around the number does not matter. Output with no digits at
all fails to parse.

A constraint is matched as a semver range when it parses as one (`>=20.0.0`,
`^1.75`). Otherwise the first version in the constraint string is treated as a
lower bound (`1.22` accepts 1.22.3). An empty string accepts anything.

## The three modes, chosen by shape

The shape of an engine's value selects the mode. The engine's name, and whether
it is well-known, change nothing.

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, or `{}`, or an object with only `versionCmd` or only `bin` | Host `PATH` | Trusts whatever is on `PATH`. Installs nothing, checks nothing, not even that the tool exists |
| A version constraint and no `installCmd` | Validate-only | Runs the version command against the host tool. Fails before any task starts if the result does not satisfy the constraint. Installs nothing |
| An `installCmd` | Provisioned | Runs `installCmd` into a content-addressed directory, version-checks the result, writes a pin, prepends its `bin` to the task's `PATH` |

Validate-only failures, verbatim:

```text
Error: engine 'node' on PATH is 26.0.0, which does not satisfy the constraint '>=99.0.0'
```

```text
Error: engine 'cabal': version command `cabal --version` failed:
sh: cabal: command not found
```

```text
Error: engine 'alpes' has a version constraint but no way to check the installed version. 'alpes' is not a well-known engine, so add a `versionCmd` to it
```

That last one is the object form's trap. `{ "version": ">=1.0.0" }` on a name
outside the well-known table passes config validation and then fails on the
first real run. Add `versionCmd` alongside `version`.

Validate-only reads each machine's own `PATH`, so a constraint that passes on
one machine and fails on another is behaving as declared. An `installCmd` is
what makes machines agree.

`--dry-run` returns before any engine is resolved, so it never reports an engine
failure. `lattice setup` reports them up front.

## Where a provisioned tool lives

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/
      pins.json
```

The directory name is the resolved version, a hyphen, and the first 8 hex
characters of `sha256(installCmd)`. `pins.json` records `engine`, `version`,
`installHash`, and `bin`.

Before installing, Lattice looks for a directory whose name ends in the current
install hash, whose `pins.json` records that hash, and whose `bin` directory
exists. When a version command and a constraint are both available it re-checks
the version, without reinstalling. A match is reused. So installation happens
once per distinct `installCmd` string, not once per run, and editing the install
command provisions into a new directory instead of reusing a stale one.

The install stages into `tmp-<hash>-<pid>-<n>` and is renamed to its final name
only after the version check passes. Nothing is left pinned on a partial failure,
so rerunning retries from scratch with nothing to clean up by hand. The pid and
the counter give two concurrent `lattice setup` runs a staging directory each,
even when both provision the same engine. Older releases shared one directory,
cleared it before use, and promoted a tree assembled from both installs. `setup`
sweeps staging left by a killed run once the directory is 24 hours old.

When no version command is available, or its output has no parseable version, the
resolved version is recorded as `unknown`, the directory is named
`unknown-<hash>`, and the cache key's toolchain component reads
`<name>=unknown@<hash>`. An older release recorded `0.0.0` instead, a version
nothing had installed, and that version went into `pins.json` and into every
cache key.

With a `version` constraint and no way to check it, provisioning is an error
rather than an unenforced constraint:

```text
Error: engine 'alpes' has the version constraint '>=2' but no way to check what was installed. 'alpes' is not a well-known engine, so add a `versionCmd` to it
```

```text
Error: engine 'alpes': could not read a version from the output of `alpes --version` after install, so the constraint '>=2.0.0' cannot be checked:
built from source
```

`installCmd` receives the target directory twice: as the `LATTICE_TOOLCHAIN_DIR`
environment variable, and by literal substitution of the text
`$LATTICE_TOOLCHAIN_DIR` into the command string before it runs. Either spelling
works, including on Windows, where the environment variable would otherwise
need `%LATTICE_TOOLCHAIN_DIR%`.

```json
{
  "engines": {
    "just": {
      "version": ">=1.30.0",
      "versionCmd": "just --version",
      "installCmd": "curl -fsSL https://just.systems/install.sh | bash -s -- --to \"$LATTICE_TOOLCHAIN_DIR/bin\"",
      "bin": "bin"
    }
  }
}
```

`bin` defaults to `"bin"` and is resolved relative to the install directory. The
value must be non-empty, relative, and inside the install. A `..` is allowed
where it does not escape, so `"bin/../libexec"` passes. An absolute `bin` would
replace the install path outright. A directory Lattice never provisioned would
go in front of every command, and the run would still report a provisioned
toolchain. Lattice checks the value when the config loads, and again
when it reads a pin back out of `pins.json`, so a hand-edited pin does not get
through either.

When a pinned directory holds the separator `PATH` is split on, `:` on unix or
`;` on Windows, it cannot go on `PATH`. That is an error, not a fallback to the
host tool:

```text
Error: the pinned toolchain cannot be put on PATH, because a directory in it contains the character PATH is split on: /repo:2/.lattice/toolchains/just/1.30.0-1a2b3c4d/bin
```

The same check runs in all three places that build a pinned `PATH`: the version
and install commands Lattice runs, `lattice setup`'s dependency installer, and
every task Lattice spawns. The first names the single directory involved. The
other two list every prepended directory, separated by `, `. On a task the check
arrives as that task's failure, with this message as the task's output, so the
run exits non-zero instead of running against the host's tool while reporting a
provisioned toolchain.

## Resolution and activation

A workspace's `engines` map merges with the root's, and the workspace wins per
key. Resolution is memoized on the merged map, so two workspaces that resolve to
the same map provision once. Under `lattice run`, only the workspaces with nodes
in the graph have their engines resolved at all. Under `lattice setup`, the root
map is always resolved, even in a repo that declares no workspaces.

Activating a toolchain means prepending its `bin` directories to the `PATH` of
the single child process Lattice spawns for one task. No shell is sourced, no
profile is written, and nothing outside `.lattice/` changes. `rm -rf
.lattice/toolchains` is a complete uninstall.

The resolved toolchain identity is part of every affected task's cache key. It
is built from one entry per engine: `<name>=host` for host mode,
`<name>=<version>@host` for validate-only, and `<name>=<version>@<installHash>`
for provisioned, sorted and joined. Changing a constraint that changes the
resolved version therefore invalidates the cache.
