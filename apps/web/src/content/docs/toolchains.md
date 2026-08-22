---
title: Toolchains
description: Every built-in task driver and every engine Lattice can version-check without help, generated from the source tables.
group: Reference
order: 3
---

# Toolchains

Lattice ships two tables. One lists the task drivers it can recognize in a
workspace and invoke a task through. The other lists the engines it knows how to
read a version from. This page is both of them in full.

For the models these tables serve, see
[Driver detection](/lattice/docs/drivers) and
[Engines and provisioning](/lattice/docs/engines).

## The built-in driver table

A driver is defined by four things: the fingerprint files that identify it in a
workspace directory, one or more roles, the command that prints its version, and
the template it invokes a task through. `{task}` is the literal placeholder each
template substitutes the task name into.

Selection follows role rank. The candidate holding the highest-ranked role
drives the workspace, a tool with several roles competes with its highest one,
and two candidates holding the same role conflict until a declaration names one.
A tool whose only role is Runtime never drives a workspace on its own.
[Driver detection](/lattice/docs/drivers) has the evidence ladder and the
composition rules.

Two drivers have no fingerprint. Nothing on disk belongs to `pip` alone or to
the Kotlin toolchain alone, so both are selected by name in `engines` or in a
`.tool-versions` file, never by detection.

### JavaScript and TypeScript

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `node` | Runtime | `.nvmrc` | `node --version` | `node {task}` |
| `deno` | Runtime, Package Manager, Task Runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` | `deno task {task}` |
| `bun` | Runtime, Package Manager | `bun.lockb`, `bun.lock` | `bun --version` | `bun run {task}` |
| `pnpm` | Package Manager | `pnpm-lock.yaml` | `pnpm --version` | `pnpm run {task}` |
| `yarn` | Package Manager | `yarn.lock` | `yarn --version` | `yarn {task}` |
| `npm` | Package Manager | `package-lock.json`, `npm-shrinkwrap.json` | `npm --version` | `npm run {task}` |
| `turbo` | Task Runner | `turbo.json` | `turbo --version` | `turbo run {task}` |
| `nx` | Task Runner | `nx.json` | `nx --version` | `nx run {task}` |

`deno` fills three roles and competes as a task runner. `bun` fills two and
competes as a package manager. `turbo` and `nx` outrank every package manager
here, so a workspace holding both `turbo.json` and `pnpm-lock.yaml` resolves to
`turbo` and composes pnpm underneath it.

### Rust

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `cargo` | Build Tool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo --version` | `cargo {task}` |

### Go

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `go` | Build Tool | `go.sum` | `go version` | `go {task}` |

### Python

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `uv` | Package Manager | `uv.lock` | `uv --version` | `uv run {task}` |
| `poetry` | Package Manager | `poetry.lock` | `poetry --version` | `poetry run {task}` |
| `pdm` | Package Manager | `pdm.lock` | `pdm --version` | `pdm run {task}` |
| `pipenv` | Package Manager | `Pipfile.lock` | `pipenv --version` | `pipenv run {task}` |
| `pip` | Package Manager | none, declaration only | `pip --version` | `pip {task}` |
| `python` | Runtime | `.python-version` | `python --version` | `python -m {task}` |

`requirements.txt` is read by pip, uv, and pip-tools alike, so it identifies no
tool and is not a `pip` fingerprint. Declare `pip` under `engines` for a
workspace pip should drive. The file still counts toward
[cache keys](/lattice/docs/cache-internals).

### Ruby

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `bundler` | Package Manager | `Gemfile.lock` | `bundle --version` | `bundle exec {task}` |
| `rake` | Task Runner | `Rakefile` | `rake --version` | `rake {task}` |
| `ruby` | Runtime | `.ruby-version` | `ruby --version` | `ruby {task}` |

### Java

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `gradle` | Build Tool | `gradlew` | `gradle --version` | `./gradlew {task}` |
| `maven` | Build Tool | `mvnw` | `mvn --version` | `./mvnw {task}` |
| `java` | Runtime | `.java-version` | `java -version` | `java {task}` |

Both build tools fingerprint the checked-in wrapper rather than the build file,
and both invoke through it. A `build.gradle` or a `pom.xml` on its own is a
generic ecosystem marker, not gradle or maven evidence.

### Kotlin

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `kotlin` | Runtime | none, declaration only | `kotlinc -version` | `kotlin {task}` |

Kotlin work is driven by gradle or maven, and no file on disk pins the Kotlin
toolchain specifically. `kotlin` is a runtime you declare and compose underneath
one of those. A `.tool-versions` entry naming `kotlin` counts as a declaration.

### .NET

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dotnet` | Build Tool | `global.json` | `dotnet --version` | `dotnet {task}` |
| `nuget` | Package Manager | `packages.config` | `nuget help` | `nuget {task}` |

`nuget` fingerprints only the legacy `packages.config` layout.
`packages.lock.json` is not nuget evidence: an SDK-style workspace can restore
with a lockfile and still be a `dotnet` workspace, and a package manager
outranks a build tool, so counting the lockfile would take the driver away from
`dotnet`. The lockfile still counts toward
[cache keys](/lattice/docs/cache-internals).

`nuget help` is the version rule because `nuget.exe` prints its version in its
help output. Lattice reads the first version-shaped substring of whatever a
version command writes to stdout or stderr, so surrounding banner text does not
matter.

### Swift and Objective-C

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `pod` | Package Manager | `Podfile`, `Podfile.lock` | `pod --version` | `pod {task}` |
| `swift` | Build Tool | `Package.resolved` | `swift --version` | `swift {task}` |

### PHP

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `composer` | Package Manager | `composer.lock` | `composer --version` | `composer {task}` |

### Elixir

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `mix` | Package Manager, Task Runner | `mix.lock` | `mix --version` | `mix {task}` |

### Dart

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dart` | Package Manager | `pubspec.lock` | `dart --version` | `dart pub {task}` |

### Haskell

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `stack` | Build Tool | `stack.yaml.lock` | `stack --version` | `stack {task}` |
| `cabal` | Build Tool | `cabal.project.freeze` | `cabal --version` | `cabal {task}` |

Two build tools with the same role. A workspace holding both lockfiles is an
ambiguity, resolved by naming one under `engines`.

### Language-agnostic task runners

| Tool | Roles | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `just` | Task Runner | `justfile`, `.justfile` | `just --version` | `just {task}` |
| `task` | Task Runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` | `task {task}` |

Neither belongs to an ecosystem. Either can sit above a language-specific driver
in any workspace.

That is the complete set: 34 drivers, across 13 ecosystems plus the two tools
above that belong to none.

## Well-known engines

An `engines` entry can be a bare version-constraint string only when Lattice has
a built-in rule for reading that tool's version. The table below is the rule set,
and it is where every driver above gets its version command. A string naming
anything outside this table is rejected by `lattice.json` validation.

```json
{ "engines": { "node": ">=20.0.0" } }
```

| Engine | Version command |
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

Forty engines. Every tool in the driver table appears here, so any driver can be
pinned in string form.

Six names are engines and not drivers: `rust`, `python3`, `php`, `elixir`,
`haskell`, and `ghc`. Each pins a compiler or interpreter that a different tool
drives tasks with. Cargo drives a Rust workspace, composer a PHP one, mix an
Elixir one, and stack or cabal a Haskell one. `haskell` and `ghc` are two
spellings of one rule. `python` and `python3` are two different interpreters and
two different rules.

A bare string is [validate-only](/lattice/docs/engines). Lattice runs the version
command against whatever is on `PATH` and fails before any task starts if the
result does not satisfy the constraint. Nothing is installed. Lattice ships no
install recipe for any engine in this table, well-known or not: provisioning
happens only when the config supplies an `installCmd`.

## Declaring a tool Lattice does not know

A name outside the table above requires the object form with an explicit
`versionCmd`. Without one, validation rejects the config:

```text
engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not
a well-known engine Lattice can version-check on its own. Use the object form
with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd":
"alpes --version" }
```

Validate a tool already on `PATH` without installing it:

```json
{
  "engines": {
    "alpes": {
      "version": ">=2.6.7",
      "versionCmd": "alpes --version"
    }
  }
}
```

Add `installCmd` to move from validate-only to provisioned. `bin` names the
directory inside the install that holds executables, and defaults to `bin`:

```json
{
  "engines": {
    "alpes": {
      "version": ">=2.6.7",
      "versionCmd": "alpes --version",
      "installCmd": "curl -fsSL https://alpes.example/install.sh | sh -s -- --dir $LATTICE_TOOLCHAIN_DIR",
      "bin": "bin"
    }
  }
}
```

Lattice then runs `installCmd` into a directory under
`.lattice/toolchains/alpes/`, version-checks the result, records a pin, and
prepends that directory's `bin` to the task's `PATH`.
[Engines and provisioning](/lattice/docs/engines) covers what
`$LATTICE_TOOLCHAIN_DIR` receives, how a pin is reused across runs, and how
`PATH` is assembled.
