# Toolchains

Lattice runs tasks with the tools a repo already declares. It never prescribes a
tool the developer did not choose, and it never mutates your global environment.
This page covers how Lattice picks a task driver, how it satisfies engine
versions, and where provisioned toolchains live on disk.

## The engine gradient

An `engine` is a versioned tool a workspace needs (a runtime, a package manager,
a build tool). Each engine constraint in `lattice.json` resolves to one of three
modes. Lattice chooses the mode from the shape of the constraint alone:

| Constraint shape | Mode | What Lattice does |
| --- | --- | --- |
| No constraint, no install command | **host PATH** | Trusts whatever is on `PATH`. Installs nothing, checks nothing. |
| A version constraint, no install command | **validate-only** | Runs the version command on the host tool and fails if it does not satisfy the constraint. Installs nothing. |
| An install command (`installCmd`) | **provision** | Runs `installCmd` into a content-addressed toolchain dir, version-checks the result, pins it, and prepends its bin dir to the task's `PATH`. |

A bare string engine (`"node": ">=20"`) is validate-only. The object form opts
into provisioning:

```json
{
  "engines": {
    "node": ">=20",
    "swift": {
      "version": ">=5.9",
      "installCmd": "swiftly install 5.9 --dir \"$LATTICE_TOOLCHAIN_DIR\"",
      "versionCmd": "swift --version",
      "bin": "bin"
    }
  }
}
```

String-form engines must be well-known (Lattice has a built-in version rule for
them). Any other tool must use the object form with an explicit `versionCmd`.

### Version checks

Validate-only and provision both version-check. An explicit `versionCmd` always
wins; otherwise Lattice uses the built-in rule for a well-known engine. Versions
are parsed tolerantly (`v20.11.1`, `go1.22`, `rustc 1.75.0 (…)`, `1.75`), and
constraints accept semver ranges (`>=20.0.0`, `^1.75`) or a bare lower bound
(`1.22`). A constraint on a tool that is neither well-known nor carries a
`versionCmd` is an error — Lattice will not guess how to check it.

## The never-prescribe evidence ladder

Detecting a runtime is not the same as detecting a **task driver** — the tool
that actually runs `build`, `test`, and friends. A lone `package.json` says
"this is JavaScript"; it does not say whether to run `pnpm run build` or
`yarn build`. Prescribing one would pick a tool the developer never chose.

So Lattice walks an evidence ladder per workspace and stops at the first rung
that gives an unambiguous answer:

1. **Declaration** — a tool named in `lattice.json` `engines`. Always wins.
2. **Native file** — a dev-authored declaration file: `packageManager` in
   `package.json`, `.tool-versions`, `.nvmrc`, `rust-toolchain.toml`,
   `./gradlew`, a `go.mod` toolchain line, and so on.
3. **Lockfile** — a tool-unique lockfile or wrapper: `bun.lockb`,
   `pnpm-lock.yaml`, `Cargo.lock`, `poetry.lock`, `composer.lock`, `turbo.json`.

If no rung produces a signal — only a bare generic marker like a lone
`package.json` or `pom.xml` — Lattice halts and asks. An `auto` workspace that
halts is a hard error with a copy-pasteable fix; a manual workspace simply has
no inferred driver.

### Roles: composition vs. conflict

Every driver carries a **role**: `Runtime`, `BuildTool`, `PackageManager`, or
`TaskRunner`. Roles are what let multiple tools coexist:

- **Different roles compose** into a stack. A node runtime (`.nvmrc`) plus a
  pnpm package manager (`pnpm-lock.yaml`) is not a conflict — pnpm drives
  because a package manager outranks a bare runtime, and node stays in the
  engine map for provisioning. A `turbo.json` above a `pnpm-lock.yaml` resolves
  to turbo: a task runner outranks a package manager.
- **The same role conflicts.** Two package managers (`pnpm-lock.yaml` and
  `bun.lockb`) or two build tools (`stack.yaml.lock` and
  `cabal.project.freeze`) in one workspace have no unique driver. Lattice raises
  an ambiguity error listing the candidates:

  ```
  Workspace 'app' has an ambiguous or undeclared task driver.
  Candidate tools seen: bun, pnpm
  Declare the task driver explicitly by adding to this workspace in lattice.json:
    "engines": { "bun": ">=0.0.0" }
  ```

  A single declaration on the higher rung breaks the tie.

Driving rank, low to high: `Runtime` < `BuildTool` < `PackageManager` <
`TaskRunner`. A pure runtime cannot drive named tasks on its own, so a workspace
whose only signal is a runtime is still ambiguous.

## On-disk layout

Everything a provisioned toolchain needs lives under `./.lattice/toolchains/`,
so `rm -rf .lattice` fully uninstalls. Nothing is written outside the repo and
no shell is sourced.

```
.lattice/toolchains/
  <engine>/
    <version>-<hash>/      # content-addressed: hash = sha256(installCmd)[:8]
      bin/                 # the `bin` dir; prepended to PATH
      pins.json
```

`pins.json` records what was installed:

```json
{
  "engine": "swift",
  "version": "5.9.2",
  "installHash": "a1b2c3d4",
  "bin": "bin"
}
```

Provisioning is memoized by content hash: an identical `installCmd` installs
once. On later runs Lattice finds the existing pin, re-verifies its version
against the constraint, and reuses it without reinstalling.

### `$LATTICE_TOOLCHAIN_DIR`

When Lattice runs an `installCmd`, it sets `$LATTICE_TOOLCHAIN_DIR` to the
staging directory for that engine and also literal-substitutes the variable into
the command string. Point your installer there — both an env-var read and a
literal `$LATTICE_TOOLCHAIN_DIR` in the command resolve to the same path. The
`bin` field names the directory (relative to that dir) that holds the
executables; it defaults to `bin`.

### Per-task PATH injection

A provisioned toolchain is activated **per task**, never globally. For each
task, Lattice clones the parent environment for that child process only and
prepends the resolved bin dirs to its `PATH`:

```
PATH = <toolchain bin dirs…> : <inherited PATH>
```

Nothing is exported to your shell, no profile is sourced, and tasks that need no
provisioned engine run with an unmodified `PATH`.

## The escape hatch: declare a bespoke tool

When a workspace uses a tool Lattice does not recognize — or a homegrown script
runner — set `auto: false` and declare everything yourself. A manual workspace
never infers commands and never halts on ambiguity; it runs exactly the
`scripts` you list, with the `engines` you provision.

```json
{
  "workspaces": [
    {
      "name": "renderer",
      "path": "services/renderer",
      "auto": false,
      "engines": {
        "blender": {
          "version": ">=4.0",
          "installCmd": "curl -sSL https://example.com/blender.tar.xz | tar -xJ -C \"$LATTICE_TOOLCHAIN_DIR\"",
          "versionCmd": "blender --version",
          "bin": "blender-4.0"
        }
      },
      "scripts": {
        "build": "blender -b scene.blend -o //out/ -a",
        "test": "./scripts/verify-frames.sh"
      }
    }
  ]
}
```

This is the same machinery the built-in drivers use, exposed directly: you name
the tool, how to install it, how to version-check it, and how each task invokes
it.

## Built-in drivers

Lattice ships an extensible registry of known task drivers. Each has a
tool-unique fingerprint (a lockfile or wrapper only that tool produces — never a
generic marker like a bare `package.json`), a role, a version command, and an
invoke form. `{task}` is the task name.

| Tool | Role | Fingerprint | Invoke form |
| --- | --- | --- | --- |
| node | Runtime | `.nvmrc` | `node {task}` |
| deno | TaskRunner | `deno.json`, `deno.jsonc`, `deno.lock` | `deno task {task}` |
| bun | PackageManager | `bun.lockb`, `bun.lock` | `bun run {task}` |
| pnpm | PackageManager | `pnpm-lock.yaml` | `pnpm run {task}` |
| yarn | PackageManager | `yarn.lock` | `yarn {task}` |
| npm | PackageManager | `package-lock.json` | `npm run {task}` |
| cargo | BuildTool | `Cargo.lock`, `rust-toolchain.toml`, `rust-toolchain` | `cargo {task}` |
| go | BuildTool | `go.sum` (+ `go.mod` toolchain line) | `go {task}` |
| uv | PackageManager | `uv.lock` | `uv run {task}` |
| poetry | PackageManager | `poetry.lock` | `poetry run {task}` |
| pdm | PackageManager | `pdm.lock` | `pdm run {task}` |
| pipenv | PackageManager | `Pipfile.lock` | `pipenv run {task}` |
| python | Runtime | `.python-version` | `python -m {task}` |
| bundler | PackageManager | `Gemfile.lock` | `bundle exec {task}` |
| rake | TaskRunner | `Rakefile` | `rake {task}` |
| ruby | Runtime | `.ruby-version` | `ruby {task}` |
| gradle | BuildTool | `gradlew` | `./gradlew {task}` |
| maven | BuildTool | `mvnw` | `./mvnw {task}` |
| java | Runtime | `.java-version` | `java {task}` |
| dotnet | BuildTool | `global.json` | `dotnet {task}` |
| pod | PackageManager | `Podfile`, `Podfile.lock` | `pod {task}` |
| composer | PackageManager | `composer.lock` | `composer {task}` |
| mix | TaskRunner | `mix.lock` | `mix {task}` |
| dart | PackageManager | `pubspec.lock` | `dart pub {task}` |
| swift | BuildTool | `Package.resolved` | `swift {task}` |
| stack | BuildTool | `stack.yaml.lock` | `stack {task}` |
| cabal | BuildTool | `cabal.project.freeze` | `cabal {task}` |
| just | TaskRunner | `justfile`, `.justfile` | `just {task}` |
| task | TaskRunner | `Taskfile.yml`, `Taskfile.yaml` | `task {task}` |
| turbo | TaskRunner | `turbo.json` | `turbo run {task}` |
| nx | TaskRunner | `nx.json` | `nx run {task}` |

For JavaScript and Deno drivers, an inferred task must exist in the manifest's
`scripts` / `tasks` map — Lattice runs what you wrote, it does not invent tasks.
Other drivers use the invoke form directly.
