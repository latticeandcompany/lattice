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
stores each result under a hash of everything that produced it, so a task whose
inputs have not changed does not run again.

It runs each workspace with the tool that workspace already uses. Its 34 drivers
cover JavaScript, Rust, Python, Go, Ruby, the JVM, .NET, Swift, PHP, Elixir,
Dart, Haskell, and two language-agnostic task runners. Lattice pins each tool's
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

`lattice init` reads the repo and shows you two lists: the directories that hold
a manifest, and the tool versions the repo already pins. Uncheck anything wrong,
and it writes `lattice.json`, a committed `.lattice/schema.json`, and the
`.gitignore` lines for what Lattice keeps locally. `lattice init --yes` skips the
prompts and writes what the scan found.

Then run a task. Below is Lattice building six of its own crates. `--filter`
picks the workspaces the run is for, and the graph still holds everything they
depend on:

```text
$ lattice run build --filter lattice-runner
lattice-events:build: running
lattice-config:build: running
lattice-events:build: done (0.20s)
lattice-config:build: done (0.22s)
lattice-cache:build: running
lattice-workspace:build: running
lattice-cache:build: done (0.14s)
lattice-workspace:build: done (0.15s)
dagger:build: running
dagger:build: done (0.87s)
lattice-runner:build: running
lattice-runner:build: done (1.74s)
lattice: 6 tasks, 0 cached, 0 failed, 3.09s
```

`lattice-cache:build` and `lattice-workspace:build` both wait on
`lattice-config:build`, because each of those workspaces names `lattice-config`
in its `dependsOn` and the `build` task depends on `^build`. `dagger:build` waits
on both of them, and `lattice-runner:build` waits on all five.

Run it again. Nothing changed, so nothing runs:

```text
$ lattice run build --filter lattice-runner
lattice-events:build: cache hit [91d1654e]
lattice-config:build: cache hit [69fddcff]
lattice-cache:build: cache hit [941d68ea]
lattice-workspace:build: cache hit [d9533b10]
dagger:build: cache hit [eb6d4dc3]
lattice-runner:build: cache hit [59afe0f7]
lattice: 6 tasks, 6 cached, 0 failed, 0.03s, 3.32s saved
lattice: full power, nothing to run
```

Both blocks are captured from this repo, which declares each of its crates as a
workspace and builds itself with Lattice.

## lattice.json

One file at the root declares the workspaces and the task graph. Every command in
it is a real shell command that you can also run by hand.

```json
{
  "latticeVersion": "1.0.0",
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=20" } },
    { "name": "api", "path": "services/api", "engines": { "go": ">=1.22" } },
    {
      "name": "utils",
      "path": "libs/utils",
      "auto": false,
      "scripts": { "build": "python3 -m build", "test": "pytest" }
    }
  ],
  "tasks": {
    "build": { "dependsOn": ["^build"], "outputs": ["dist/**"] },
    "test": { "dependsOn": ["build"] },
    "dev": { "persistent": true, "cache": false }
  }
}
```

`web` and `api` are detected: Lattice reads the lockfile or declaration file
already in each directory and runs the tool that file identifies. `utils` sets
`auto: false` and declares its own commands.

In `dependsOn`, `^build` means `build` in this workspace's dependencies. A bare
task name, like `build` under `test`, means that task in this same workspace.

## How it works

A task's cache key is a hash of everything that can change what the task
produces: the command it resolved to, the files in its workspace, the manifests
and lockfiles the workspace and the repo root hold, the environment values it
names, the toolchain it resolved to, and the cache keys of the tasks it depends
on. Declaring `inputs` narrows the file set to the globs you list. Change one
file and every task downstream of it runs again, and nothing else does.

A stored result counts as a hit only when its metadata parses and the archive's
sha256 matches the digest recorded when it was written, so a corrupt artifact is
a miss and never a false hit.

Every run appends what its hits skipped to a ledger kept beside the cache, and
`lattice stats` adds those lines up. The figure is task time rather than wall
clock: each hit contributes the time the run that wrote its entry spent.

A workspace left on `auto` gets its tool from the evidence in its own directory:
a declaration file such as `packageManager` or `rust-toolchain.toml`, or a
lockfile only one tool produces. Lattice then calls that tool, whether that is
`npm`, `pnpm`, `cargo`, `go`, `uv`, `poetry`, `bundler`, `gradle`, `maven`,
`dotnet`, or one of the others. If two tools of the same kind both have evidence,
the run stops before any task starts and prints the `engines` line that settles
it. A subtree with its own task runner becomes one workspace whose script calls
that runner.

An `engines` entry takes a version constraint, and the shape of the constraint
decides what Lattice does with it. A version with no `installCmd` checks the tool
already on `PATH` and fails the run if the version does not satisfy the
constraint. A constraint with an `installCmd` installs the tool under
`./.lattice/toolchains/`, records the version it resolved, and prepends that
install's `bin` directory to the `PATH` of each task that needs it.

Persistent tasks, such as dev servers, stream their output line by line and are
never cached.

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
shape of other config files. [`skills/lattice/`](../skills/lattice/SKILL.md) is a
skill that replaces the guess. It carries the commands, the fields, the driver
and engine tables, and the failures a run produces:

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
