---
title: Toolchains
description: The complete built-in driver table and well-known engine list, regenerated from source.
group: Reference
order: 3
---

# Toolchains

This is the exhaustive reference for the tools Lattice knows about out of the
box: every built-in task driver, and every engine it can version-check without
help. The models behind these tables are [Driver detection](/lattice/docs/drivers)
(how a workspace's task driver gets picked) and [Engines and
provisioning](/lattice/docs/engines) (how a version constraint gets satisfied).
Read those for the *why*; this page is the lookup.

## The built-in driver table

Each driver is a `DriverSpec`: a fingerprint that identifies it in a workspace
directory, a `Role`, the command that prints its version, and the shell
template it invokes a task with (`crates/lattice-workspace/src/lib.rs:70-288`).
A driver's role decides how it competes: different roles in the same workspace
compose into a stack (a node runtime plus a pnpm package manager); two drivers
with the *same* role conflict until you disambiguate. A pure `Runtime` — node,
python, ruby, java below — never drives a workspace by itself; on its own it
still halts as an ambiguous/undetected driver, the same as no evidence at all.
See [Driver detection](/lattice/docs/drivers) for the full evidence ladder and
the composition/conflict rule.

`{task}` below is the literal placeholder each driver substitutes the task
name into.

### JavaScript and TypeScript

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `node` | Runtime | `.nvmrc` | `node --version` | `node {task}` |
| `deno` | Task Runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` | `deno task {task}` |
| `bun` | Package Manager | `bun.lockb`, `bun.lock` | `bun --version` | `bun run {task}` |
| `pnpm` | Package Manager | `pnpm-lock.yaml` | `pnpm --version` | `pnpm run {task}` |
| `yarn` | Package Manager | `yarn.lock` | `yarn --version` | `yarn {task}` |
| `npm` | Package Manager | `package-lock.json` | `npm --version` | `npm run {task}` |

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
| `python` | Runtime | `.python-version` | `python --version` | `python -m {task}` |

### Ruby

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `bundler` | Package Manager | `Gemfile.lock` | `bundle --version` | `bundle exec {task}` |
| `rake` | Task Runner | `Rakefile` | `rake --version` | `rake {task}` |
| `ruby` | Runtime | `.ruby-version` | `ruby --version` | `ruby {task}` |

### The JVM (Java, Gradle, Maven)

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `gradle` | Build Tool | `gradlew` | `gradle --version` | `./gradlew {task}` |
| `maven` | Build Tool | `mvnw` | `mvn --version` | `./mvnw {task}` |
| `java` | Runtime | `.java-version` | `java -version` | `java {task}` |

### .NET

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dotnet` | Build Tool | `global.json` | `dotnet --version` | `dotnet {task}` |

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
| `mix` | Task Runner | `mix.lock` | `mix --version` | `mix {task}` |

### Dart and Flutter

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `dart` | Package Manager | `pubspec.lock` | `dart --version` | `dart pub {task}` |

### Haskell

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `stack` | Build Tool | `stack.yaml.lock` | `stack --version` | `stack {task}` |
| `cabal` | Build Tool | `cabal.project.freeze` | `cabal --version` | `cabal {task}` |

### Generic task runners

These aren't tied to one language — any of them can sit above a
language-specific driver in a workspace (see [Driver
detection](/lattice/docs/drivers) on composition).

| Tool | Role | Fingerprint | Version command | Invoke template |
| --- | --- | --- | --- | --- |
| `just` | Task Runner | `justfile`, `.justfile` | `just --version` | `just {task}` |
| `task` | Task Runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` | `task {task}` |
| `turbo` | Task Runner | `turbo.json` | `turbo --version` | `turbo run {task}` |
| `nx` | Task Runner | `nx.json` | `nx --version` | `nx run {task}` |

That is the complete `DRIVERS` table: 31 drivers across 13 language and
ecosystem groups (`crates/lattice-workspace/src/lib.rs:70-288`).

## Well-known engines

An `engines` entry can be a bare version-constraint string only if Lattice
already has a built-in rule for checking that tool's version — the
`WELL_KNOWN_ENGINES` list (`crates/lattice-config/src/lib.rs:13-16`). A string
naming anything else is rejected by `lattice.json` validation. Every name
below is accepted in the short string form:

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
| `python3` | `python --version` |
| `ruby` | `ruby --version` |
| `bundler` | `bundle --version` |
| `java` | `java -version` |
| `gradle` | `gradle --version` |
| `maven` | `mvn --version` |
| `dotnet` | `dotnet --version` |

(`crates/lattice-workspace/src/toolchain.rs:35-70`, `builtin_version_cmd`.)
`rust` has no entry in the driver table above — cargo is the task driver for a
Rust workspace, but `rust` is a separate, valid engine name for pinning the
`rustc` toolchain version itself.

A bare string is shorthand for a version-only constraint — no `installCmd`
means [validate-only](/lattice/docs/engines): Lattice runs the version command
above against whatever is already on `PATH` and fails if it doesn't satisfy
the constraint. It never installs anything for a well-known engine unless you
add an explicit `installCmd` in the object form.

## Declaring a tool Lattice doesn't know

Any other tool name needs the object form, with an explicit `versionCmd` —
skip it and `lattice.json` validation rejects the config with the exact fix
(`crates/lattice-config/src/lib.rs:304-316`):

```text
engine 'alpes' in root uses the string (version-only) form, but 'alpes' is not
a well-known engine Lattice can version-check on its own. Use the object form
with an explicit `versionCmd`, e.g. "alpes": { "version": ">=1.0.0", "versionCmd":
"alpes --version" }
```

A working example, validating a tool already on `PATH` without installing it:

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

Add `installCmd` (and optionally `bin`, which defaults to `bin`) to move this
from validate-only to provisioned — Lattice installs it into
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

The full mechanics of that gradient — what `$LATTICE_TOOLCHAIN_DIR` receives,
how a pin is reused across runs, and what gets prepended to a task's `PATH` —
are on the [Engines and provisioning](/lattice/docs/engines) page.
