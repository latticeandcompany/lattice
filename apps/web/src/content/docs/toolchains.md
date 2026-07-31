---
title: Toolchains
description: The complete built-in driver table and well-known engine list, regenerated from source.
group: Reference
order: 3
---

# Toolchains

The exhaustive reference for the tools Lattice knows out of the box: every
built-in task driver, and every engine it can version-check without help. For the
models behind these tables see [Driver detection](/lattice/docs/drivers) and
[Engines and provisioning](/lattice/docs/engines).

## The built-in driver table

Every built-in driver is defined by a fingerprint that identifies it in a
workspace directory, one or more roles, the command that prints its version, and
the template it invokes a task with. The tables below are generated from that
definition set in the source. `{task}` is the literal placeholder each driver
substitutes the task name into.

The candidate with the highest-ranked role drives a workspace; a tool with
several roles competes with its highest one; two candidates holding the same role
conflict until a declaration names one. A pure runtime — `node`, `python`,
`ruby`, `java`, `kotlin` below — never drives a workspace on its own. See [Driver
detection](/lattice/docs/drivers) for the evidence ladder and the full rule.

Two drivers have no fingerprint: nothing on disk belongs to `pip` or to the Kotlin
toolchain alone, so both are selected by naming them in `engines` or in a
`.tool-versions` file, never by detection.

### JavaScript and TypeScript

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `node` | Runtime | `.nvmrc` | `node --version` | `node {task}` |
| `deno` | Runtime, Package Manager, Task Runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` | `deno task {task}` |
| `bun` | Runtime, Package Manager | `bun.lockb`, `bun.lock` | `bun --version` | `bun run {task}` |
| `pnpm` | Package Manager | `pnpm-lock.yaml` | `pnpm --version` | `pnpm run {task}` |
| `yarn` | Package Manager | `yarn.lock` | `yarn --version` | `yarn {task}` |
| `npm` | Package Manager | `package-lock.json`, `npm-shrinkwrap.json` | `npm --version` | `npm run {task}` |

### Rust

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `cargo` | Build Tool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo --version` | `cargo {task}` |

### Go

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `go` | Build Tool | `go.sum` | `go version` | `go {task}` |

### Python

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `uv` | Package Manager | `uv.lock` | `uv --version` | `uv run {task}` |
| `poetry` | Package Manager | `poetry.lock` | `poetry --version` | `poetry run {task}` |
| `pdm` | Package Manager | `pdm.lock` | `pdm --version` | `pdm run {task}` |
| `pipenv` | Package Manager | `Pipfile.lock` | `pipenv --version` | `pipenv run {task}` |
| `pip` | Package Manager | none — declaration only | `pip --version` | `pip {task}` |
| `python` | Runtime | `.python-version` | `python --version` | `python -m {task}` |

A `requirements.txt` is read by pip, uv, and pip-tools alike, so it names no tool
and is not a pip fingerprint. Declare `pip` in `engines` for a workspace you want
pip to drive.

### Ruby

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `bundler` | Package Manager | `Gemfile.lock` | `bundle --version` | `bundle exec {task}` |
| `rake` | Task Runner | `Rakefile` | `rake --version` | `rake {task}` |
| `ruby` | Runtime | `.ruby-version` | `ruby --version` | `ruby {task}` |

### The JVM (Java, Kotlin, Gradle, Maven)

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `gradle` | Build Tool | `gradlew` | `gradle --version` | `./gradlew {task}` |
| `maven` | Build Tool | `mvnw` | `mvn --version` | `./mvnw {task}` |
| `java` | Runtime | `.java-version` | `java -version` | `java {task}` |
| `kotlin` | Runtime | none — declaration only | `kotlinc -version` | `kotlin {task}` |

A Kotlin project is driven by gradle or maven, and no file on disk pins the Kotlin
toolchain specifically, so `kotlin` is a runtime you declare and compose
underneath one of those. A `.tool-versions` entry naming `kotlin` counts as a
declaration.

### .NET

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dotnet` | Build Tool | `global.json` | `dotnet --version` | `dotnet {task}` |
| `nuget` | Package Manager | `packages.config` | `nuget help` | `nuget {task}` |

`nuget` fingerprints only the legacy `packages.config` layout. A
`packages.lock.json` is not nuget evidence: an SDK-style project can restore with
a lockfile and still be a `dotnet` workspace, and since a package manager outranks
a build tool, counting it would take the driver away from `dotnet`. The lockfile
still counts toward [cache keys](/lattice/docs/cache-internals).

### Swift and Objective-C

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `pod` | Package Manager | `Podfile`, `Podfile.lock` | `pod --version` | `pod {task}` |
| `swift` | Build Tool | `Package.resolved` | `swift --version` | `swift {task}` |

### PHP

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `composer` | Package Manager | `composer.lock` | `composer --version` | `composer {task}` |

### Elixir

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `mix` | Package Manager, Task Runner | `mix.lock` | `mix --version` | `mix {task}` |

### Dart

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dart` | Package Manager | `pubspec.lock` | `dart --version` | `dart pub {task}` |

### Haskell

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `stack` | Build Tool | `stack.yaml.lock` | `stack --version` | `stack {task}` |
| `cabal` | Build Tool | `cabal.project.freeze` | `cabal --version` | `cabal {task}` |

### Generic task runners

Not tied to one language. Any of them can sit above a language-specific driver in
a workspace.

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `just` | Task Runner | `justfile`, `.justfile` | `just --version` | `just {task}` |
| `task` | Task Runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` | `task {task}` |
| `turbo` | Task Runner | `turbo.json` | `turbo --version` | `turbo run {task}` |
| `nx` | Task Runner | `nx.json` | `nx --version` | `nx run {task}` |

That is the complete built-in driver set: 34 drivers across 13 language and
ecosystem groups.

## Well-known engines

An `engines` entry can be a bare version-constraint string only if Lattice has a
built-in rule for reading that tool's version. The table below is that rule set,
and it is where every driver above gets its version command too. A string naming
anything else is rejected by `lattice.json` validation. Every name below is
accepted in the short form:

```json
{ "engines": { "node": ">=20.0.0" } }
```

| Engine | Version rule (command Lattice runs) |
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

Every tool in the driver table appears here, so any driver can be pinned in string
form. Six names are engines but not drivers: `rust`, `python3`, `php`, `elixir`,
`haskell`, and `ghc`. Each pins a compiler or interpreter that some other tool
drives tasks with — cargo drives a Rust workspace, composer PHP, mix Elixir, stack
or cabal Haskell. `haskell` and `ghc` are two spellings of one rule; `python` and
`python3` are two different interpreters.

`nuget help` is the version rule because nuget.exe has no `--version` flag; its
help output prints `NuGet Version: x.y.z` on the first line. Lattice reads the
first version-looking substring of whatever a version command prints, so banner
text around it doesn't matter.

A bare string means [validate-only](/lattice/docs/engines): Lattice runs the
version command above against whatever is on `PATH` and fails if it doesn't
satisfy the constraint. It installs nothing for a well-known engine unless you add
an `installCmd` in the object form.

## Declaring a tool Lattice doesn't know

Any other tool name needs the object form with an explicit `versionCmd`. Skip it
and `lattice.json` validation rejects the config with the exact fix:

```text
engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not
a well-known engine Lattice can version-check on its own. Use the object form
with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd":
"alpes --version" }
```

Validating a tool already on `PATH`, without installing it:

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

Add `installCmd` (and optionally `bin`, which defaults to `bin`) to move from
validate-only to provisioned. Lattice then installs into
`.lattice/toolchains/alpes/` and pins the result instead of trusting `PATH`:

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

[Engines and provisioning](/lattice/docs/engines) covers what
`$LATTICE_TOOLCHAIN_DIR` receives, how a pin is reused across runs, and what gets
prepended to a task's `PATH`.
