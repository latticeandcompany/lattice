# Discovery, drivers, tasks, engines, and provisioning

A **driver** is the tool that runs a workspace's tasks. An **engine** is a
versioned tool named under `engines`. The two are resolved independently: every
driver is also a well-known engine name, but naming an engine never selects a
driver.

## Which directories are workspaces

1. The repo root's workspace files, expanded into members:

   | File | What is read | A member must hold |
   | --- | --- | --- |
   | `package.json` | `workspaces` (array, or `{ "packages": [...] }`) | `package.json` |
   | `pnpm-workspace.yaml` | the `packages:` list | `package.json` |
   | `Cargo.toml` | `[workspace] members`, minus `exclude` | `Cargo.toml` |
   | `go.work` | `use ./dir` and `use ( ... )` | `go.mod` |
   | `pyproject.toml` | `[tool.uv.workspace] members`, minus `exclude` | `pyproject.toml` |
   | `deno.json`, `deno.jsonc` | `workspace` | `deno.json`, `deno.jsonc`, or `package.json` |

   A `!pattern` entry removes what the entries before it matched. The root is a
   member too when `Cargo.toml` has `[package]` beside `[workspace]`, or
   `pyproject.toml` has `[project]` beside `[tool.uv.workspace]`.
2. `lattice.json` `workspaces`: directories or globs added on top.
3. `lattice.json` `exclude`: globs of workspace paths removed from both.
4. With nothing from 1 or 2, the root alone.

No other file is expanded. Gradle's `settings.gradle`, Maven's `<modules>`, a
`.sln` and a nested workspace file all belong to the workspace they sit in,
which runs as one: its tool orders its own subprojects, and two builds of one
Gradle or MSBuild project at once contend for the same locks and outputs.

A directory a glob matched that has nothing Lattice recognizes and no override
command is not a workspace. A listed directory that resolves outside the repo
through a symlink fails discovery.

## How a driver is chosen

An override's `driver` wins outright, and `"none"` means no driver. Otherwise,
three passes, stopping at the first that decides:

### 1. Evidence in the workspace

| Evidence | What counts |
| --- | --- |
| Native file | `packageManager` in `package.json`; a tool in `.tool-versions` or under `[tools]` in `mise.toml`/`.mise.toml`; `.nvmrc`; `rust-toolchain.toml` or `rust-toolchain`; `.python-version`; a `[tool.poetry]`, `[tool.pdm]` or `[tool.uv]` table in `pyproject.toml`; `.ruby-version` or a `ruby` line in `Gemfile`; `.java-version`; a `toolchain` line in `go.mod`; `gradlew`; `mvnw`; `deno.json`/`deno.jsonc` |
| Lockfile | Any other fingerprint in the driver table below |

Each driver competes with its highest-ranked role:

| Role | Rank | Drivers |
| --- | --- | --- |
| Runtime | 0 | `node`, `python`, `ruby`, `java`, `kotlin` |
| Build tool | 1 | `cargo`, `go`, `gradle`, `maven`, `dotnet`, `swift`, `stack`, `cabal` |
| Package manager | 2 | `pnpm`, `yarn`, `npm`, `bun`, `uv`, `poetry`, `pdm`, `pipenv`, `pip`, `bundler`, `nuget`, `pod`, `composer`, `dart` |
| Task runner | 3 | `just`, `task`, `turbo`, `nx`, `deno`, `rake`, `mix` |

Take the candidates at the highest rank present. If that rank is above
Runtime, the one with the strongest evidence (native file over lockfile) drives;
two tied for strongest halt. A runtime alone cannot drive a named task, so it
decides nothing and the next pass runs.

### 2. The nearest directory above, for JS and Python

A workspace whose manifest could belong to several tools — `package.json` (npm,
pnpm, yarn, bun) or `pyproject.toml` (uv, poetry, pdm, pipenv) — and that has no
evidence of its own walks up toward the repo root. The nearest directory with
evidence for one of those tools decides, by the same rules. This is how a member
of a pnpm workspace, whose only lockfile is at the root, resolves to pnpm.

### 3. A manifest only one tool reads

| Manifest | Driver |
| --- | --- |
| `Cargo.toml` | `cargo` |
| `go.mod` | `go` |
| `Pipfile` | `pipenv` |
| `requirements.txt`, `setup.py` | `pip` |
| `Gemfile` | `bundler` |
| `pom.xml` | `maven` |
| `build.gradle`, `build.gradle.kts`, `settings.gradle`, `settings.gradle.kts` | `gradle` |
| `composer.json` | `composer` |
| `mix.exs` | `mix` |
| `pubspec.yaml` | `dart` |
| `Package.swift` | `swift` |
| `stack.yaml` | `stack` |
| `cabal.project`, a `.cabal` file | `cabal` |
| a `.sln`, `.slnx`, `.csproj`, `.fsproj` or `.vbproj` file | `dotnet` |

Manifests are checked in the order `package.json`, `Cargo.toml`, `go.mod`,
`pyproject.toml`, `Pipfile`, `requirements.txt`, `setup.py`, `Gemfile`,
`pom.xml`, the Gradle files, `composer.json`, `mix.exs`, `pubspec.yaml`,
`Package.swift`, `stack.yaml`, `cabal.project`, then .NET and `.cabal` files;
the first one present decides which tools are in play. A directory holding both
a `package.json` and a `Cargo.toml`, with no lockfile anywhere, is a JS
workspace that halts rather than a Cargo one.

### When nothing decides

```text
error: Lattice cannot tell which of npm, pnpm, yarn, bun runs the tasks in workspace 'gen': there is no lockfile or packageManager field to go by.
Say which in lattice.json:
  "overrides": { "gen": { "driver": "npm" } }
```

Two tools tied for strongest evidence produce the same message naming them. A
directory matched by `lattice.json` `workspaces` that holds nothing recognizable
is skipped, not an error; a member the root's workspace files name that holds
nothing recognizable produces the other form, whose fix writes out a command:

```text
error: Lattice cannot tell what runs the tasks in workspace 'x': it has no lockfile, wrapper or manifest Lattice recognizes.
Say which in lattice.json:
  "overrides": { "x": { "tasks": { "build": "<command>" } } }
```

Worked cases:

- `.nvmrc` plus `pnpm-lock.yaml`: `pnpm`. The runtime stays an engine.
- `packageManager: "pnpm@9.1.0"` plus a stray `package-lock.json`: `pnpm`, a
  native file over a lockfile.
- `pnpm-lock.yaml` plus `bun.lockb`: halts, two package managers on equal
  evidence.
- `turbo.json` plus `pnpm-lock.yaml`: `turbo`, a task runner over a package
  manager.
- A pnpm member with no lockfile of its own: `pnpm`, from the root's lockfile.
- A bare `Cargo.toml`, `go.mod` or `build.gradle.kts`: `cargo`, `go`, `gradle`.
- `Gemfile` plus `Rakefile`: `rake`, run as `bundle exec rake <task>`.

## The driver table

34 drivers. `Language` is the slug a front end keys a language mark off; `just`
and `task` carry none.

| Tool | Language | Roles | Fingerprint | Version command |
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
| `just` | none | Task runner | `justfile`, `.justfile` | `just --version` |
| `task` | none | Task runner | `Taskfile.yml`, `Taskfile.yaml` | `task --version` |

## The tasks each driver can run

A task runs in a workspace when one of these defines it, or an override writes
it out. Nothing else is invented, and a workspace without a task sits the run
out. Where a script map and a verb share a name, the script map wins.

| Driver | Tasks |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun` | `package.json` `scripts`: `npm run <x>`, `pnpm run <x>`, `yarn run <x>`, `bun run <x>` |
| `deno` | `deno.json`/`deno.jsonc` `tasks` (`deno task <x>`); `test` `deno test`, `lint` `deno lint`, `format` `deno fmt` |
| `composer` | `composer.json` `scripts`: `composer run-script <x>` |
| `cargo` | `build`, `test`, `check`, `doc`, `bench` as `cargo <x>`; `lint` `cargo clippy`; `format` `cargo fmt`; `clean` `cargo clean -p <package>` |
| `go` | `build` `go build ./...`, `test` `go test ./...`, `check` and `lint` `go vet ./...`, `format` `go fmt ./...`, `clean` `go clean` |
| `gradle` | `build`, `assemble`, `check`, `test`, `clean` through the wrapper; `run` when the build applies `application`; `bootRun` with Spring Boot; `format` `spotlessApply` and `lint` `spotlessCheck` with Spotless; `lint` with the Android plugin; any task the scripts register (`tasks.register("x")`, `tasks.create("x")`, `task x`) |
| `maven` | `build` `package -DskipTests`, `test`, `check` `verify`, `install`, `clean`; `format` `spotless:apply` and `lint` `spotless:check` with the Spotless plugin |
| `dotnet` | `build`, `test`, `clean`, `pack`; `format` `dotnet format`; `lint` `dotnet format --verify-no-changes` |
| `swift` | `build`, `test`; `clean` `swift package clean` |
| `stack` | `build`, `test`, `clean` |
| `cabal` | `build` `cabal build all`, `test` `cabal test all`, `clean` |
| `dart` | `test`; `check` and `lint` `analyze`; `format` `dart format .` — through `flutter` when `pubspec.yaml` has `sdk: flutter` |
| `mix` | `build` `mix compile`, `test`, `format`, `clean`; `lint` `mix credo` when `mix.exs` mentions `:credo` |
| `uv`, `poetry`, `pdm` | `build` `<tool> build`; `pdm` also runs `[tool.pdm.scripts]` (`pdm run <x>`) |
| `pipenv` | Pipfile `[scripts]`: `pipenv run <x>` |
| `uv`, `poetry`, `pdm`, `pipenv`, `pip` | By declared dependency: `test` with `pytest`; `lint` and `format` with `ruff` (`ruff check .`, `ruff format .`), else `format` with `black`, `lint` with `flake8`; `check` with `mypy`. Run as `uv run …`, `poetry run …`, `pdm run …`, `pipenv run …`, or bare for `pip` (`python -m pytest`) |
| `bundler`, `rake` | Rakefile tasks (`task :x`, `task x:`, `task "x"`) as `rake <x>`, under `bundle exec` when there is a `Gemfile`; `test` `rake test` with a `Rake::TestTask`, else `rspec` when the Gemfile names it; `lint` `rubocop` when named |
| `just` | justfile recipes: `just <x>` |
| `task` | Taskfile `tasks:` keys: `task <x>` |
| `turbo` | `turbo.json` `tasks` (or `pipeline`) keys, minus `pkg#task` entries: `turbo run <x>` |
| `nx` | `nx.json` `targetDefaults` keys: `nx run-many -t <x>` |
| `node`, `python`, `ruby`, `java`, `kotlin`, `pod`, `nuget` | Nothing. Write tasks out in an override |

Python dependencies are read from `[project] dependencies`,
`optional-dependencies`, `[dependency-groups]`, `[tool.pdm.dev-dependencies]`,
Poetry's dependency tables and groups, Pipfile `packages`/`dev-packages`, and
every `requirements*.txt`.

The wrapper is the nearest `gradlew` (`gradlew.bat` on Windows) or `mvnw` in the
workspace or any directory above it inside the repo, called relatively
(`./gradlew`, `../gradlew`); with none, `gradle` or `mvn` on `PATH`.

A manifest that will not parse runs none of its scripts and warns:
`web: package.json could not be parsed: <reason>, so it runs none of its
scripts`. `deno.json`, `deno.jsonc` and `turbo.json` are read as JSONC;
`package.json` is strict JSON.

### Defaults that come with a task

| Task | Default |
| --- | --- |
| `build` | `dependsOn: ["^build"]` |
| `dev`, `start`, `serve`, `watch` | `persistent: true` |
| `clean`, `deploy`, `publish`, `release` | `cache: false` |

A `build` the workspace's own files define also gets its tool's build directory
as `outputs`:

| Driver | Default `build` outputs |
| --- | --- |
| `npm`, `pnpm`, `yarn`, `bun`, `deno`, `uv`, `poetry`, `pdm` | `dist/**` |
| `gradle` | `build/**` beside each `build.gradle(.kts)` |
| `maven` | `target/**` beside each `pom.xml` |
| `dotnet` | `**/bin/Debug/**`, `**/bin/Release/**` |
| anything else, and a uv workspace member (it builds into the root's `dist/`) | none |

The root `tasks` map and overrides replace any of these field by field.

### Arguments after `--`

`lattice run <task> -- <args>` appends the arguments, each quoted for the shell
unless it is plain (`[A-Za-z0-9-_./=:@%+,]`), to the named task's command. A
script run through `npm run` gets npm's `--` first. Everything else gets them
directly, so a tool's own separator still has to be written:
`lattice run test -- -- --nocapture` for Cargo's test harness.

## Well-known engines

A bare constraint string is accepted only for these 40 names; any other needs
the object form with `versionCmd`.

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
| `gradle` | `gradle --version`, or `<wrapper> --version` in the workspace |
| `maven` | `mvn --version`, or `<wrapper> --version` in the workspace |
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

A `gradle` or `maven` engine with no `versionCmd` is checked through the
build's wrapper, run in the workspace, because the wrapper decides which Gradle
or Maven the build uses; a machine with no `gradle` on `PATH` passes a
wrapper-only build's check.

Version parsing takes the first run of digits and dots in the output
(`v20.11.1`, `go1.22`, `rustc 1.75.0 (abc 2024)`). A constraint that parses as a
semver range is matched as one (`>=20.0.0`, `^1.75`; a bare `22.11.0` is
`^22.11.0`). Otherwise its first version is a lower bound.

## The three modes, chosen by shape

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, `{}`, or an object with only `versionCmd` or `bin` | Host | Whatever is on `PATH`. Nothing checked |
| A constraint and no `installCmd` | Validate | Runs the version command on the host before any task starts; fails if it does not satisfy the constraint |
| An `installCmd` | Provision | Installs once into `.lattice/toolchains`, checks the version, pins it, puts its `bin` on every task's `PATH` |

Checks and installs happen on the first real run that needs them (`lattice run`,
`lattice setup`, `lattice env`), never on `--dry-run`. A run notes what it uses:
`using node 22.11.0 · cargo 1.90.0 (installed by Lattice) · just (unchecked)`.

### What a failure says

Every engine failure names what was wanted, what was found and where, the
command that ran, what it printed, and a fix. Inside a run it is wrapped in
`workspace '<name>' cannot run`:

```text
error: workspace 'web' cannot run
  engine 'node' does not match the version this repo pins
    wanted   >=22
    found    20.11.1 at /usr/local/bin/node
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

| Headline | Cause |
| --- | --- |
| `is not installed, or not on PATH` | The shell could not find the program (exit 127, or `is not recognized` on Windows) |
| `does not match the version this repo pins` | Validate mode: the host version fails the constraint |
| `could not be checked: its version command failed` | The command ran and exited non-zero; its output follows |
| `could not be checked: Lattice found no version in what it printed` | No digits in the output |
| `has a version constraint but no way to check the installed version` | A name with no built-in rule and no `versionCmd` |
| `could not be installed: its installCmd exited with code <n>` | The install failed; the command and the last 20 lines of its output follow |
| `was installed, but its version command failed` | The installed tool is not where `bin` says, or does not run |
| `installed a version that does not match its constraint` | The install worked but pinned the wrong version |
| `was installed, but Lattice could not read a version from it` | A constraint cannot be checked against what was installed |
| `has the constraint '<c>' but no way to check what was installed` | Provision mode with a constraint and no version rule |

## Where a provisioned tool lives

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/
      pins.json
```

The directory is the resolved version, a hyphen, and the first 8 hex characters
of `sha256(installCmd)`. `pins.json` records `engine`, `version`,
`installHash`, and `bin`. A directory with the current hash, a matching pin and
an existing `bin` is reused, after re-checking the version when a constraint and
a version command exist, so an install runs once per distinct `installCmd`.
Editing the command provisions a new directory.

The install stages into `tmp-<hash>-<pid>-<n>` and is renamed only after its
version check passes, so a failed install leaves nothing pinned. Staging left by
a killed run is swept after 24 hours. With no readable version the directory is
`unknown-<hash>` and the identity reads `<name>=unknown@<hash>`.

`installCmd` gets its target directory as `LATTICE_TOOLCHAIN_DIR` and by literal
substitution of `$LATTICE_TOOLCHAIN_DIR` in the command, so either spelling
works on every platform. It should put the tool there with its executables under
`bin` (or wherever `bin` says).

```json
{
  "engines": {
    "just": {
      "version": ">=1.30.0",
      "installCmd": "curl -fsSL https://just.systems/install.sh | bash -s -- --to \"$LATTICE_TOOLCHAIN_DIR/bin\""
    }
  }
}
```

`bin` must be relative, non-empty, and inside the install, checked at load and
again when a pin is read back. A pinned directory holding a character `PATH`
cannot carry (`:` on unix, `"` on Windows) is an error, never a fall-back to the
host tool.

## Resolution and activation

A workspace's engines are the root map with its overrides' engines layered per
name. Resolution is memoized on that map (and, for `gradle`/`maven`, on the
workspace directory), so identical maps provision once. Under `lattice run`, only
workspaces with nodes in the graph are resolved.

Activation prepends the toolchain `bin` directories to the `PATH` of the one
child process a task runs in; nothing outside `.lattice/` changes, and `rm -rf
.lattice/toolchains` uninstalls everything. The project's dependency bin
directories go on after them. `lattice env` prints the same `PATH` for your own
shell.

The toolchain identity is part of every affected cache key: `<name>=host`,
`<name>=<version>@host`, or `<name>=<version>@<installHash>`, sorted and joined.
