# Drivers, engines, and provisioning

## How a driver is chosen

A workspace's task `build` has to become a real shell command. The tool that
supplies it is the workspace's **driver**. Lattice climbs a fixed ladder per
workspace and stops at the first rung that produces a single, unambiguous tool:

| Rung | Evidence | Example |
| --- | --- | --- |
| 1. Declaration | A tool named in this workspace's or the root's `engines` | `"engines": { "pnpm": ">=8" }` |
| 2. Native file | A file the developer authored to pin a tool | `packageManager` in `package.json`, `.tool-versions`, `mise.toml` `[tools]`, `.nvmrc`, `rust-toolchain.toml`, a `go.mod` `toolchain` line, `./gradlew`, `.python-version`, `.java-version`, `.ruby-version` |
| 3. Lockfile | A lockfile or wrapper only one tool produces | `pnpm-lock.yaml`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, `turbo.json` |
| 4. Nothing unambiguous | — | halt and ask |

Rung 1 wins outright. Below it, native files and lockfiles only disambiguate
when two tools would otherwise tie.

A bare generic marker is never enough on its own: a lone `package.json`,
`pom.xml`, `pyproject.toml`, or `Gemfile` identifies an ecosystem, not a tool.
Those names appear only in the candidate list an ambiguity error suggests.

### Roles: what composes, what conflicts

Every driver carries one or more roles. Different roles in one workspace compose
into a stack; the highest rank drives. Two drivers competing for the *same*
driving role conflict and halt.

| Role | Rank | Examples |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet` |
| Package manager | 2 | `pnpm`, `npm`, `yarn`, `bun`, `uv`, `poetry`, `nuget` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake` |

A tool can fill several roles: `deno` is a runtime, a package manager, and a task
runner, and `bun` is a runtime and a package manager. Such a tool drives with its
highest-ranked role — `deno` as a task runner, `bun` as a package manager — so a
`.nvmrc` beside a `bun.lockb` still resolves to bun rather than tying with node.

A `.nvmrc` beside a `pnpm-lock.yaml` is not a conflict: pnpm drives, and node
stays on record in the resolved engine map so it can still be provisioned. Two
package managers — `pnpm-lock.yaml` and `bun.lockb` — have no tiebreak and halt.

A pure runtime can never drive a named task by itself; there is no universal
`node build`. A workspace whose only evidence is runtime-level halts exactly as
if there were no evidence at all:

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "auto": false, "scripts": { "build": "<command>" }
```

### `auto: false` turns the ladder's consequences off

Lattice may still notice a driver, but it never infers a command from one and an
ambiguous or undetectable result never halts the run. Only `scripts` supplies
commands. A task requested against such a workspace with no matching entry is
fatal rather than skipped.

## The built-in driver table

`{task}` is the placeholder each driver substitutes the task name into.

### JavaScript and TypeScript

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `node` | Runtime | `.nvmrc` | `node --version` | `node {task}` |
| `deno` | Runtime, Package Manager, Task Runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` | `deno task {task}` |
| `bun` | Runtime, Package Manager | `bun.lockb`, `bun.lock` | `bun --version` | `bun run {task}` |
| `pnpm` | Package Manager | `pnpm-lock.yaml` | `pnpm --version` | `pnpm run {task}` |
| `yarn` | Package Manager | `yarn.lock` | `yarn --version` | `yarn {task}` |
| `npm` | Package Manager | `package-lock.json`, `npm-shrinkwrap.json` | `npm --version` | `npm run {task}` |

### Rust, Go, Python

`pip` has no fingerprint of its own: a `requirements.txt` is read by pip, uv, and
pip-tools alike, so naming pip in `engines` is the only way to select it.

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `cargo` | Build Tool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo --version` | `cargo {task}` |
| `go` | Build Tool | `go.sum` | `go version` | `go {task}` |
| `uv` | Package Manager | `uv.lock` | `uv --version` | `uv run {task}` |
| `poetry` | Package Manager | `poetry.lock` | `poetry --version` | `poetry run {task}` |
| `pdm` | Package Manager | `pdm.lock` | `pdm --version` | `pdm run {task}` |
| `pipenv` | Package Manager | `Pipfile.lock` | `pipenv --version` | `pipenv run {task}` |
| `pip` | Package Manager | none — declaration only | `pip --version` | `pip {task}` |
| `python` | Runtime | `.python-version` | `python --version` | `python -m {task}` |

### Ruby, the JVM, .NET

`kotlin` has no fingerprint either: a Kotlin project is driven by gradle or
maven, so the Kotlin toolchain is pinned as an engine and composes underneath.
`nuget` fingerprints only the legacy `packages.config` layout — an SDK-style
project can carry a `packages.lock.json` and still be a `dotnet` workspace.

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `bundler` | Package Manager | `Gemfile.lock` | `bundle --version` | `bundle exec {task}` |
| `rake` | Task Runner | `Rakefile` | `rake --version` | `rake {task}` |
| `ruby` | Runtime | `.ruby-version` | `ruby --version` | `ruby {task}` |
| `gradle` | Build Tool | `gradlew` | `gradle --version` | `./gradlew {task}` |
| `maven` | Build Tool | `mvnw` | `mvn --version` | `./mvnw {task}` |
| `java` | Runtime | `.java-version` | `java -version` | `java {task}` |
| `kotlin` | Runtime | none — declaration only | `kotlinc -version` | `kotlin {task}` |
| `dotnet` | Build Tool | `global.json` | `dotnet --version` | `dotnet {task}` |
| `nuget` | Package Manager | `packages.config` | `nuget help` | `nuget {task}` |

### Swift, PHP, Elixir, Dart, Haskell

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `pod` | Package Manager | `Podfile`, `Podfile.lock` | `pod --version` | `pod {task}` |
| `swift` | Build Tool | `Package.resolved` | `swift --version` | `swift {task}` |
| `composer` | Package Manager | `composer.lock` | `composer --version` | `composer {task}` |
| `mix` | Package Manager, Task Runner | `mix.lock` | `mix --version` | `mix {task}` |
| `dart` | Package Manager | `pubspec.lock` | `dart --version` | `dart pub {task}` |
| `stack` | Build Tool | `stack.yaml.lock` | `stack --version` | `stack {task}` |
| `cabal` | Build Tool | `cabal.project.freeze` | `cabal --version` | `cabal {task}` |

### Generic task runners

Not tied to one language; any of these can sit above a language-specific driver.

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `just` | Task Runner | `justfile`, `.justfile` | `just --version` | `just {task}` |
| `task` | Task Runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` | `task {task}` |
| `turbo` | Task Runner | `turbo.json` | `turbo --version` | `turbo run {task}` |
| `nx` | Task Runner | `nx.json` | `nx --version` | `nx run {task}` |

That is the complete set: 34 drivers across 13 ecosystem groups.

## How a task's command resolves

An explicit entry in a workspace's `scripts` map always wins. Only a task with
no entry there falls back to the driver's inferred invocation. Two limits on
inference are worth knowing:

- For JavaScript-family drivers, inference requires the task name to exist in
  the manifest's own `scripts`/`tasks` map. Lattice never invents a script the
  `package.json` doesn't have.
- For direct-invoke drivers (`cargo`, `go`, and similar), inference never
  fabricates a command for a `persistent` task — there is no universal
  `cargo dev` — so a persistent task on one of those needs an explicit
  `scripts` entry.

## Well-known engines

An `engines` value may be a bare constraint string only for a name Lattice has a
built-in version rule for. Anything else must use the object form with an
explicit `versionCmd`, or the config is rejected on load.

Every tool in the driver table above is a well-known engine, so any driver can be
declared in string form. A handful of names appear here only — they pin a compiler
or interpreter that some other tool drives tasks with.

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

`rust`, `php`, `elixir`, and `haskell`/`ghc` have no entry in the driver table —
cargo, composer, mix, and stack/cabal drive those workspaces, while these names
pin the language toolchain itself. `nuget help` is the version rule because
nuget.exe has no `--version` flag; it prints `NuGet Version: x.y.z` first.

Lattice parses the first version-looking substring of whatever a version command
prints, so extra banner text does not matter.

## The three modes, chosen by shape

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, or `{}` | host `PATH` | Trusts whatever is on `PATH`. Installs nothing, checks nothing |
| A version constraint, no `installCmd` | validate-only | Runs the version command against the host tool, fails if it doesn't satisfy the constraint. Installs nothing |
| An `installCmd` | provisioned | Runs `installCmd` into a content-addressed directory, version-checks the result, pins it, prepends its `bin` to the task's `PATH` |

Nothing else changes the mode — not the engine's name, not whether it's
well-known. A validate-only failure looks like:

```text
engine 'node' 18.19.0 on PATH does not satisfy constraint '>=20.0.0'
```

Because validate-only trusts each machine's own `PATH`, a constraint that passes
on one machine and fails on another is working as declared. Moving it to
provisioned mode is what makes machines agree.

## Where a provisioned tool lives

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/          # prepended to the task's PATH
      pins.json
```

The directory is named for the resolved version plus the first 8 hex characters
of `sha256(installCmd)`. `pins.json` records `engine`, `version`, `installHash`,
and `bin`. Before installing, Lattice reuses an existing directory whose
`pins.json` matches the current `installCmd` hash and whose `bin` still exists,
re-checking the version where a version command is available. Installation
happens once per distinct `installCmd` content.

While `installCmd` runs there is no resolved version yet, so the install builds
into a temporary `tmp-<hash>` directory and is renamed only after the new tool
passes its version check. Nothing is left pinned on partial failure, so
rerunning retries from scratch with no state to clean up by hand.

Hashing the literal `installCmd` string means changing the install command — a
different registry, a different flag — provisions into a new directory instead of
silently reusing a stale one.

`installCmd` receives the target directory twice: as the `LATTICE_TOOLCHAIN_DIR`
environment variable, and by literal substitution of `$LATTICE_TOOLCHAIN_DIR`
into the command string before it runs. Either spelling works:

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

## Resolution and activation

Provisioning and version resolution happen once per workspace, memoized: two
workspaces resolving to the same merged engine map provision once. *Activating* a
toolchain — putting its `bin` on `PATH` — happens per task, in the environment of
the one child process spawned for it. No shell is sourced and no profile is
written, so `rm -rf .lattice/toolchains` is a complete uninstall.

A workspace's `engines` map merges with the root's, and the workspace wins per
key. The merge happens before any task in that workspace runs, under both
`lattice run` and `lattice setup`.
