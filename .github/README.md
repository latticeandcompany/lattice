<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/lockup-white.svg">
  <img alt="Lattice" src="assets/lockup-black.svg" width="320">
</picture>

<br />

**A high-performance, local toolchain for managing monorepos.**

<br />

![Version](https://img.shields.io/github/v/release/latticeandcompany/lattice?label=version&include_prereleases&sort=semver)
[![CI](https://img.shields.io/github/actions/workflow/status/latticeandcompany/lattice/ci.yml?branch=mega&label=CI&logo=githubactions&logoColor=white)](https://github.com/latticeandcompany/lattice/actions/workflows/ci.yml)
![Rust](https://img.shields.io/badge/Rust-1.88+-000000?logo=rust&logoColor=white)
![License](https://img.shields.io/github/license/latticeandcompany/lattice)

</div>

---

Lattice runs the tasks in your repo in dependency order and in parallel. It
reads the workspaces, the dependencies between them and their tasks from the
files your tools already read — `pnpm-workspace.yaml`, Cargo's `[workspace]`,
`go.work`, each package's scripts, a Gradle build — so a `lattice.json` holds
only what those cannot say. It stores each result under a hash of everything
that produced it, so a task whose inputs have not changed does not run again.

It runs each workspace with the tool that workspace already uses, across
JavaScript, Rust, Python, Go, Ruby, the JVM, .NET, Swift, PHP, Elixir, Dart,
Haskell, and two language-agnostic task runners. It can also pin each tool's
version, so every machine builds against the same one.

## Install

Run this from the root of the repo you want to use Lattice in:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

That covers macOS, Linux, and Windows under Git Bash, MSYS2, or Cygwin. In
PowerShell:

```powershell
irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
```

The installer checks the download against its published sha256 and writes the
binary to `./.lattice/bin/`. It then adds `.lattice/bin/` to `.gitignore`, if the
directory has one, and adds `.lattice/bin` to `PATH` in your shell config, so
`lattice` in this repo means the version this repo pins. To skip the `PATH` edit
and run the binary as `./.lattice/bin/lattice` instead:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

`install.ps1` adds `.lattice\bin` to your user `PATH` rather than to a shell
config, and asks first when it has a terminal to ask on. Set
`$env:LATTICE_NO_PATH = '1'` to skip that.

Or through the package manager the repo already uses:

```sh
npm install --save-dev @latticeandcompany/lattice
```

npm unpacks one prebuilt binary, the same one the release publishes, and puts
`lattice` on your package scripts. There is no download step and no `postinstall`
script.

An npm install is the one that does not follow a `latticeVersion` pin. Your
lockfile has already chosen the version, so Lattice runs what npm installed and
warns when the two disagree.

[Installation](https://latticeandcompany.github.io/lattice/docs/installation)
covers all three in full.

To uninstall a script install, delete `.lattice` and drop the `PATH` entry: the
`lattice` line from the shell config the installer names when it finishes, or
the `.lattice\bin` entry in your user `PATH` on Windows.

If the repo already has a `lattice.json`, you get the version its
`latticeVersion` names. `lattice upgrade 0.2.0` moves the repo to another version
and writes that version back to `lattice.json`. Commit the change and everyone on
the repo moves at once.

To build from source instead, run
`cargo install --git https://github.com/latticeandcompany/lattice lattice`, or
clone the repo and run `cargo build --release`. Either way you need Rust 1.88 or
later.

## Quick start

```sh
lattice init
lattice run build
```

`lattice init` reads the repo and writes a `lattice.json` holding only what it
would not find on its own: projects no workspace file names, and the tool
versions the repo already pins. Then it lists every workspace it sees, with the
tool that runs it and the tasks it can run:

```text
✓ wrote lattice.json
✓ wrote .lattice/schema.json
✓ updated .gitignore

2 workspaces:
  app    npm      build test
  greet  npm      build

next: lattice run build
```

In that two-package npm repo, the whole config is a version pin. `app` depends
on `greet` in its `package.json`, so `greet` builds first:

```text
$ lattice run build | cat
greet:build: running
greet:build: done (0.54s)
app:build: running
app:build: done (0.44s)
lattice: 2 tasks, 0 cached, 0 failed, 1.12s
```

Run it again. Nothing changed, so nothing runs:

```text
$ lattice run build | cat
greet:build: cache hit [11330699]
app:build: cache hit [3f0a3810]
lattice: 2 tasks, 2 cached, 0 failed, 0.01s, 0.98s saved
lattice: full power, nothing to run
```

On a terminal, the same run draws a live display: each running task with the
last lines it printed, the tasks waiting and what they wait on, and progress.

## lattice.json

Every key is optional. A monorepo whose packages sit in one workspace file needs
nothing beyond a version. A repo with several languages says what no single
manifest can:

```json
{
  "latticeVersion": "2.0.0",
  "workspaces": ["services/*", "tools/codegen"],
  "engines": { "node": ">=22", "go": ">=1.23" },
  "tasks": {
    "test": { "dependsOn": ["build"] }
  },
  "overrides": {
    "web": { "dependsOn": ["api"] },
    "tools/codegen": { "name": "codegen", "driver": "none", "tasks": { "build": "make proto" } }
  }
}
```

`workspaces` adds directories to the ones the root's workspace files already
name. `overrides` holds what belongs to particular workspaces, keyed by name or
path: an edge between languages, a command written out by hand, a tool Lattice
cannot pick on its own.

A task runs wherever it exists: a `package.json` script, a justfile recipe, a
Rakefile task, or a verb the workspace's tool has — `lattice run lint` is
`cargo clippy` in a crate and `go vet ./...` in a module. A workspace without the
task sits the run out. In `dependsOn`, `^build` means `build` in the workspaces
this one depends on; `build` depends on `^build` by default.

Moving from 1.x: `lattice migrate` rewrites the old config, keeping every
workspace's name.

## How it works

A task's cache key is a hash of everything that can change what the task
produces: the command it resolved to, the files in its workspace, the manifests
and lockfiles the workspace and the repo root hold, the environment values it
names, the tool versions it resolved to, and the cache keys of the tasks it
depends on. Change one file and every task downstream of it runs again, and
nothing else does. A stored result counts as a hit only when its archive's
sha256 matches the digest recorded when it was written, so a corrupt artifact is
a miss and never a false hit.

Each workspace's tool comes from the evidence in its files: a `packageManager`
field, a lockfile only one tool writes, a wrapper like `gradlew`, a manifest only
one tool reads. A package inside a pnpm monorepo takes pnpm from the lockfile at
the root. When nothing decides, the run stops before any task starts and prints
the line that settles it. A Gradle, Maven or .NET build is one workspace, and
its own tool orders its subprojects.

An `engines` entry either checks the tool already on `PATH` or, with an
`installCmd`, installs the tool under `./.lattice/toolchains/` and puts it on the
`PATH` of each task. A tool that is wrong stops the run with the version wanted,
the one found and where, the command that ran, and the fix. `lattice env` prints
the same `PATH` for your own shell.

Every task runs with an empty stdin and no controlling terminal, so a tool that
reads the terminal fails with its own error instead of hanging the run.

## Documentation

The full documentation is at
[latticeandcompany.github.io/lattice/docs](https://latticeandcompany.github.io/lattice/docs).
Start with
[getting started](https://latticeandcompany.github.io/lattice/docs/getting-started),
then read
[configuration](https://latticeandcompany.github.io/lattice/docs/configuration),
[caching](https://latticeandcompany.github.io/lattice/docs/caching),
[toolchains](https://latticeandcompany.github.io/lattice/docs/toolchains), and
the [CLI reference](https://latticeandcompany.github.io/lattice/docs/cli).

## For agents

Lattice is new enough that a coding agent will guess at `lattice.json` from the
shape of other config files, listing every workspace by hand.
[`skills/lattice/`](../skills/lattice/SKILL.md) is a skill that replaces the
guess. It carries the discovery rules, the commands, the fields, the driver and
engine tables, and the failures a run produces:

```bash
npx skills add latticeandcompany/lattice
```

See [for-agents](https://latticeandcompany.github.io/lattice/for-agents) for what
the skill contains and how to load it for a single session instead.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first. It covers setup, the build and
test commands, branching, the testing requirements, and the AI-disclosure rule.
The crate layout is in
[architecture](https://latticeandcompany.github.io/lattice/docs/architecture).
Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md).
Vulnerabilities go through [SECURITY.md](SECURITY.md), never a public issue.

## License

ISC © [Ryan Mullin](https://github.com/hiteacheryouare) and contributors

---

<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/latticeco-white.png">
  <img alt="Lattice &amp; Company" src="assets/latticeco-black.png" width="240">
</picture>

<br />
<br />

<sub>© 2026 Lattice &amp; Co.</sub>

</div>
