---
title: Toolchains
description: Every built-in driver, the tasks each one can run, what setup installs with it, and every engine Lattice can version-check without help.
group: Reference
order: 3
---

# Toolchains

This page is Lattice's built-in tables in full: the drivers it can recognize, the
tasks each one can run, the installer `lattice setup` uses with each, and the
engines it knows how to read a version from. For the models these tables serve,
see [Driver detection](/lattice/docs/drivers) and [Engines and
provisioning](/lattice/docs/engines).

## The built-in driver table

A driver is recognized by its fingerprint files, competes by its role, and has a
command that prints its version. The candidate with the highest-ranked role
drives a workspace; [Driver detection](/lattice/docs/drivers) has the rules.

| Tool | Ecosystem | Roles | Fingerprint | Version command |
| --- | --- | --- | --- | --- |
| `node` | node | Runtime | `.nvmrc` | `node --version` |
| `deno` | node | Runtime, Package manager, Task runner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno --version` |
| `bun` | node | Runtime, Package manager | `bun.lockb`, `bun.lock` | `bun --version` |
| `pnpm` | node | Package manager | `pnpm-lock.yaml` | `pnpm --version` |
| `yarn` | node | Package manager | `yarn.lock` | `yarn --version` |
| `npm` | node | Package manager | `package-lock.json`, `npm-shrinkwrap.json` | `npm --version` |
| `turbo` | node | Task runner | `turbo.json` | `turbo --version` |
| `nx` | node | Task runner | `nx.json` | `nx --version` |
| `cargo` | rust | Build tool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo --version` |
| `go` | go | Build tool | `go.sum` | `go version` |
| `uv` | python | Package manager | `uv.lock` | `uv --version` |
| `poetry` | python | Package manager | `poetry.lock` | `poetry --version` |
| `pdm` | python | Package manager | `pdm.lock` | `pdm --version` |
| `pipenv` | python | Package manager | `Pipfile.lock` | `pipenv --version` |
| `pip` | python | Package manager | none | `pip --version` |
| `python` | python | Runtime | `.python-version` | `python --version` |
| `bundler` | ruby | Package manager | `Gemfile.lock` | `bundle --version` |
| `rake` | ruby | Task runner | `Rakefile` | `rake --version` |
| `ruby` | ruby | Runtime | `.ruby-version` | `ruby --version` |
| `gradle` | java | Build tool | `gradlew` | `gradle --version` |
| `maven` | java | Build tool | `mvnw` | `mvn --version` |
| `java` | java | Runtime | `.java-version` | `java -version` |
| `kotlin` | kotlin | Runtime | none | `kotlinc -version` |
| `dotnet` | dotnet | Build tool | `global.json` | `dotnet --version` |
| `nuget` | dotnet | Package manager | `packages.config` | `nuget help` |
| `pod` | swift | Package manager | `Podfile`, `Podfile.lock` | `pod --version` |
| `swift` | swift | Build tool | `Package.resolved` | `swift --version` |
| `composer` | php | Package manager | `composer.lock` | `composer --version` |
| `mix` | elixir | Package manager, Task runner | `mix.lock` | `mix --version` |
| `dart` | dart | Package manager | `pubspec.lock` | `dart --version` |
| `stack` | haskell | Build tool | `stack.yaml.lock` | `stack --version` |
| `cabal` | haskell | Build tool | `cabal.project.freeze` | `cabal --version` |
| `just` | — | Task runner | `justfile`, `.justfile` | `just --version` |
| `task` | — | Task runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` |

A fingerprint is not the only way in. A manifest only one tool reads is enough
on its own — `Cargo.toml`, `go.mod`, `pom.xml`, a `build.gradle`, a `.csproj`, a
`Gemfile` — and a `package.json` or `pyproject.toml` workspace with no evidence
of its own takes its tool from the nearest directory above it that has one. `pip`
and `kotlin` have no fingerprint because no file belongs to them alone.

## Tasks each driver can run

A task runs in a workspace only where one of these defines it, or an override
writes it out. Where a script and a built-in verb share a name, the script wins.

| Driver | Tasks |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun` | Every `package.json` script, as `npm run <x>`, `pnpm run <x>`, `yarn run <x>`, `bun run <x>` |
| `deno` | Every task in `deno.json`, as `deno task <x>`; `test` (`deno test`), `lint` (`deno lint`), `format` (`deno fmt`) |
| `turbo` | Every task in `turbo.json`, as `turbo run <x>` |
| `nx` | Every `targetDefaults` key in `nx.json`, as `nx run-many -t <x>` |
| `composer` | Every `composer.json` script, as `composer run-script <x>` |
| `just` | Every justfile recipe, as `just <x>` |
| `task` | Every Taskfile task, as `task <x>` |
| `cargo` | `build`, `test`, `check`, `doc`, `bench`; `lint` (`cargo clippy`), `format` (`cargo fmt`), `clean` (`cargo clean -p <crate>`) |
| `go` | `build` (`go build ./...`), `test` (`go test ./...`), `check` and `lint` (`go vet ./...`), `format` (`go fmt ./...`), `clean` |
| `gradle` | `build`, `assemble`, `check`, `test`, `clean`; `run` with the `application` plugin; `bootRun` with Spring Boot; `format` (`spotlessApply`) and `lint` (`spotlessCheck`) with Spotless; `lint` with the Android plugin; any task the build script registers |
| `maven` | `build` (`package -DskipTests`), `test`, `check` (`verify`), `install`, `clean`; `format` (`spotless:apply`) and `lint` (`spotless:check`) with Spotless |
| `dotnet` | `build`, `test`, `clean`, `pack`; `format` (`dotnet format`), `lint` (`dotnet format --verify-no-changes`) |
| `swift` | `build`, `test`, `clean` (`swift package clean`) |
| `stack` | `build`, `test`, `clean` |
| `cabal` | `build` (`cabal build all`), `test` (`cabal test all`), `clean` |
| `dart` | `test`, `check` and `lint` (`analyze`), `format` — through `flutter` for a Flutter app |
| `mix` | `build` (`mix compile`), `test`, `format`, `clean`; `lint` (`mix credo`) when `mix.exs` uses Credo |
| `uv`, `poetry`, `pdm` | `build` (`uv build`, `poetry build`, `pdm build`); pdm also runs `[tool.pdm.scripts]` |
| `pipenv` | Every Pipfile script, as `pipenv run <x>` |
| `uv`, `poetry`, `pdm`, `pipenv`, `pip` | `test` when `pytest` is a dependency; `lint` and `format` with `ruff` (else `format` with `black`, `lint` with `flake8`); `check` with `mypy`. Through the tool's `run` (`uv run pytest`), or `python -m pytest` for pip |
| `bundler`, `rake` | Every Rakefile task, under `bundle exec` when there is a `Gemfile`; `test` with a `Rake::TestTask`, else `rspec` when the Gemfile names it; `lint` (`rubocop`) when named |
| `node`, `python`, `ruby`, `java`, `kotlin`, `pod`, `nuget` | None. Write the tasks out in an override |

Gradle and Maven commands run through the nearest `gradlew` or `mvnw` in the
workspace or any directory above it, or through `gradle` and `mvn` when there is
no wrapper.

### Defaults that come with a task

| Task | Default |
| --- | --- |
| `build` | Depends on `^build` |
| `dev`, `start`, `serve`, `watch` | Persistent |
| `clean`, `deploy`, `publish`, `release` | Never cached |

A `build` a workspace's own files define also caches its tool's build directory:

| Driver | Default `build` outputs |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun`, `deno`, `uv`, `poetry`, `pdm` | `dist/**` |
| `gradle` | `build/**` beside each `build.gradle(.kts)` |
| `maven` | `target/**` beside each `pom.xml` |
| `dotnet` | `**/bin/Debug/**`, `**/bin/Release/**` |
| every other driver, and a uv workspace member | none |

The `tasks` map and overrides replace any of these. See
[Configuration](/lattice/docs/configuration#defaults-and-layering).

## What `lattice setup` installs

| Driver | Installer |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun` | `npm install`, `pnpm install`, `yarn install`, `bun install` |
| `deno` | `deno cache .` |
| `cargo` | `cargo fetch` |
| `go` | `go mod download` |
| `uv`, `poetry`, `pdm`, `pipenv` | `uv sync`, `poetry install`, `pdm install`, `pipenv install` |
| `pip` | `pip install -r requirements.txt` |
| `bundler` | `bundle install` |
| `gradle` | `./gradlew dependencies`, or `gradle dependencies` with no wrapper |
| `maven` | `./mvnw dependency:resolve`, or `mvn dependency:resolve` with no wrapper |
| `dotnet`, `nuget` | `dotnet restore`, `nuget restore` |
| `pod`, `swift` | `pod install`, `swift package resolve` |
| `composer`, `mix`, `dart` | `composer install`, `mix deps.get`, `dart pub get` |
| `stack`, `cabal` | `stack build --only-dependencies`, `cabal build --only-download` |

The runtimes and the task runners have no installer; their workspaces get their
engines checked and nothing else.

## Well-known engines

An `engines` entry can be a bare constraint string only for these forty names,
because Lattice knows how to read their versions:

```json
{ "engines": { "node": ">=22" } }
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
| `gradle` | `gradle --version`, or the build's wrapper |
| `maven` | `mvn --version`, or the build's wrapper |
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

A `gradle` or `maven` engine without its own `versionCmd` is checked through the
build's wrapper, run inside the workspace, because the wrapper decides which
Gradle or Maven the build uses. A machine with no `gradle` on `PATH` passes a
wrapper build's check.

Six names are engines and not drivers — `rust`, `python3`, `php`, `elixir`,
`haskell`, `ghc` — because a different tool drives their workspaces. Naming an
engine never selects a driver; `overrides.<ws>.driver` does that.

Lattice ships no install recipe for any of these. It installs a tool only when
the config gives it an `installCmd`.

## Declaring a tool Lattice does not know

A name outside the table needs the object form with a `versionCmd`:

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

Add `installCmd` to have Lattice install it. `bin` names the directory inside the
install that holds executables, and defaults to `bin`:

```json
{
  "engines": {
    "alpes": {
      "version": ">=2.6.7",
      "versionCmd": "alpes --version",
      "installCmd": "curl -fsSL https://alpes.example/install.sh | sh -s -- --dir $LATTICE_TOOLCHAIN_DIR"
    }
  }
}
```

[Engines and provisioning](/lattice/docs/engines) covers what
`$LATTICE_TOOLCHAIN_DIR` receives, how an install is reused, and what a failed
check says.
