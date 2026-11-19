---
title: Driver detection
description: How Lattice picks the tool that runs a workspace's tasks, what that tool can run, and when it stops to ask.
group: Concepts
order: 4
---

# Driver detection

A task named `build` has to become a real shell command. The tool that supplies
that command is the workspace's **driver**: `pnpm`, `cargo`, `gradle`, `just`.

The file that identifies a language does not always identify a tool. A
`package.json` says JavaScript. It does not say whether `build` means
`pnpm run build`, `yarn run build`, `npm run build` or `bun run build`, and
those four resolve different dependency trees. Lattice looks for evidence that
names one tool, and stops to ask when there is none.

## Three places to look

Lattice checks three places in order and stops at the first that decides.

### 1. The workspace itself

| Evidence | Examples |
| --- | --- |
| A file a developer wrote to name a tool | `packageManager` in `package.json`, `.tool-versions`, a `[tools]` table in `mise.toml`, a `[tool.poetry]`, `[tool.pdm]` or `[tool.uv]` table in `pyproject.toml`, `gradlew`, `mvnw`, `deno.json`, `.nvmrc`, `rust-toolchain.toml` |
| A lockfile only one tool writes | `pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`, `package-lock.json`, `Cargo.lock`, `uv.lock`, `poetry.lock`, `turbo.json`, `justfile` |

When several tools are in evidence, their **roles** decide:

| Role | Rank | Examples |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift`, `stack`, `cabal` |
| Package manager | 2 | `pnpm`, `npm`, `yarn`, `bun`, `uv`, `poetry`, `pdm`, `pipenv`, `pip`, `bundler`, `composer`, `dart` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake`, `mix` |

The highest-ranked role drives, so different roles compose rather than
conflict. A `.nvmrc` beside a `pnpm-lock.yaml` is pnpm, with node kept as an
engine to check. A `turbo.json` over a `pnpm-lock.yaml` is turbo, because the
task runner outranks the package manager and runs the tasks the repo declared to
it. Among tools of the same role, a file someone wrote on purpose beats a
lockfile: `packageManager: "pnpm@9.1.0"` wins over a stray `package-lock.json`.

A runtime alone decides nothing, because there is no universal `node build`.

### 2. The directory above, for JavaScript and Python

A package inside a pnpm monorepo has no lockfile of its own; pnpm wrote one at
the root. So a workspace whose `package.json` or `pyproject.toml` could belong
to several tools, and that has no evidence of its own, looks upward. The nearest
directory with a lockfile or a `packageManager` for one of those tools decides.
This is why the members of a JavaScript monorepo need no configuration.

### 3. A manifest only one tool reads

`Cargo.toml` is Cargo's and `go.mod` is Go's. A manifest like that is enough on
its own:

| Manifest | Driver |
| --- | --- |
| `Cargo.toml` | `cargo` |
| `go.mod` | `go` |
| `pom.xml` | `maven` |
| `build.gradle`, `build.gradle.kts`, `settings.gradle` | `gradle` |
| `.csproj`, `.fsproj`, `.vbproj`, `.sln` | `dotnet` |
| `Gemfile` | `bundler` |
| `Pipfile` | `pipenv` |
| `requirements.txt`, `setup.py` | `pip` |
| `composer.json`, `mix.exs`, `pubspec.yaml`, `Package.swift`, `stack.yaml`, `cabal.project` | `composer`, `mix`, `dart`, `swift`, `stack`, `cabal` |

## When Lattice stops to ask

A `package.json` or `pyproject.toml` with nothing to go by, here or above, halts
the run before anything starts:

```text
error: Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

Two package managers tied on equal evidence, `pnpm-lock.yaml` beside
`bun.lockb`, halt the same way, naming both.

Lattice could pick one. Picking one means every repo that never chose a package
manager silently gets whichever Lattice's authors preferred, and the day the
guess is wrong the failure looks like a bug in your build. Committing the
lockfile settles it, and so does naming the tool:

```json
{ "overrides": { "gen": { "driver": "pnpm" } } }
```

`driver` also overrides a detection you disagree with: `"driver": "pnpm"` in a
workspace that has a `turbo.json` runs pnpm scripts instead of `turbo run`. The
value is any driver in the [toolchain reference](/lattice/docs/toolchains), or
`"none"` for a workspace whose every task you write out yourself.

## What a driver can run

Knowing the driver is not the same as knowing the commands. Lattice runs a task
in a workspace only where something defines it, and invents nothing.

**Tools with a script map run what the map declares.** A `package.json`'s
`scripts`, deno's `tasks`, a `composer.json`'s `scripts`, justfile recipes,
Taskfile tasks, Rakefile tasks, a `turbo.json`'s tasks. `lattice run lint` in an
npm workspace with no `lint` script does not run there.

**Build tools run the verbs they have.** They publish no list of tasks, so
Lattice knows each one's real verbs:

| Driver | `build` | `test` | `lint` | Others |
| --- | --- | --- | --- | --- |
| `cargo` | `cargo build` | `cargo test` | `cargo clippy` | `check`, `format` (`cargo fmt`), `doc`, `bench`, `clean` (`cargo clean -p <crate>`) |
| `go` | `go build ./...` | `go test ./...` | `go vet ./...` | `check`, `format` (`go fmt ./...`), `clean` |
| `gradle` | `./gradlew build` | `./gradlew test` | with Spotless or Android | `assemble`, `check`, `clean`, `run` with `application`, `bootRun` with Spring Boot, any task the build script registers |
| `maven` | `./mvnw package -DskipTests` | `./mvnw test` | with Spotless | `check` (`verify`), `install`, `clean` |
| `dotnet` | `dotnet build` | `dotnet test` | `dotnet format --verify-no-changes` | `format`, `pack`, `clean` |

**Some verbs need proof.** A Python workspace gets `test` only when `pytest` is a
dependency, `lint` and `format` only with `ruff` (or `black`, `flake8`), `check`
only with `mypy`. A Ruby workspace gets `lint` only when its Gemfile names
`rubocop`. Offering `pytest` to a project that uses something else would be a
command that fails.

The complete table is in the [toolchain
reference](/lattice/docs/toolchains#tasks-each-driver-can-run).

A Gradle or Maven command goes through the build's wrapper, found in the
workspace or any directory above it (`./gradlew`, `../gradlew`), or `gradle`
and `mvn` on `PATH` when there is none.

## A missing task is skipped, a typo is flagged

A workspace without the task sits the run out, and the run carries on with the
workspaces that have it. `lattice run lint` means "lint everything that lints"
without a list of which those are. The live display says who sat out:

```text
· no 'lint' task in docs, web, so they sit this one out
```

A missing task and a mistyped one look the same from outside, so a workspace
that declares a name one typo away is a warning on every run:

```text
lattice: warning: web has no 'build' task. Did you mean 'biuld'?
```

A task no workspace has at all refuses to run, with the tasks that do exist:

```text
error: no workspace has a 'tset' task. Did you mean 'test'?
Tasks: build, check, clean, dev, format, lint, test
```

## Writing a command yourself

An override's `tasks` supplies a command directly, for a workspace whose
manifest does not declare the task or whose tool has no such verb:

```json
{
  "overrides": {
    "api": { "tasks": { "deploy": "./deploy.sh", "jib": "./gradlew jib" } }
  }
}
```

A written-out command replaces what Lattice found for that task in that
workspace. Everything else keeps coming from the manifest.
