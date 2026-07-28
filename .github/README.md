<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/lockup-white.svg">
  <img alt="Lattice" src="assets/lockup-black.svg" width="320">
</picture>

<br />

**A high-performance, local toolchain for managing monorepos.**

<br />

![Rust](https://img.shields.io/badge/Rust-1.75+-000000?logo=rust&logoColor=white)
![Cargo](https://img.shields.io/badge/Cargo-workspace-000000?logo=rust&logoColor=white)
![Astro](https://img.shields.io/badge/Astro-7-BC52EE?logo=astro&logoColor=white)
![Bootstrap](https://img.shields.io/badge/Bootstrap-5-7952B3?logo=bootstrap&logoColor=white)
![TailwindCSS](https://img.shields.io/badge/TailwindCSS-4-06B6D4?logo=tailwindcss&logoColor=white)

![Version](https://img.shields.io/badge/version-0.1.0-informational)
![License](https://img.shields.io/github/license/latticeandcompany/lattice)
[![CI](https://img.shields.io/github/actions/workflow/status/latticeandcompany/lattice/ci.yml?branch=mega&label=CI&logo=githubactions&logoColor=white)](https://github.com/latticeandcompany/lattice/actions/workflows/ci.yml)
![Stars](https://img.shields.io/github/stars/latticeandcompany/lattice?logo=github)
![Forks](https://img.shields.io/github/forks/latticeandcompany/lattice?logo=github)
![Issues](https://img.shields.io/github/issues/latticeandcompany/lattice?logo=github)
![Last Commit](https://img.shields.io/github/last-commit/latticeandcompany/lattice?logo=git&logoColor=white)

</div>

---

Lattice runs the tasks in your monorepo in dependency order and in parallel, then caches every result by content so unchanged work is skipped. It reads the tools each project already declares, across JavaScript, Rust, Python, Go, Ruby, the JVM and .NET, and pins those versions so everyone builds against the same ones.

## Install

Lattice is pre-release, so install it from source:

```sh
cargo install --git https://github.com/latticeandcompany/lattice lattice
```

Or clone and build:

```sh
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build --release
```

<sub>A `curl | sh` installer that drops a target-matched binary into `./.lattice/bin/` ships with the first tagged release.</sub>

## Quick start

```sh
lattice init          # writes lattice.json
lattice run build
```

`lattice init` asks what the repo needs Lattice for, which directories are workspaces, and which tool versions to pin, then writes `lattice.json` alongside its JSON schema. `lattice init --yes` skips the questions and writes a minimal skeleton. Then run a task:

```
worker:build: running
web:build: running
api:build: running
utils:build: running
worker:build: done (0.01s)
api:build: done (0.02s)
web:build: done (0.40s)
utils:build: done (0.40s)
lattice: 4 tasks, 0 cached, 0 failed, 0.52s
```

Run it again. Nothing changed, so nothing runs:

```
worker:build: cache hit [bff6d3d0]
utils:build: cache hit [795cc88d]
web:build: cache hit [4d4c6d35]
api:build: cache hit [e5f9de2e]
lattice: 4 tasks, 4 cached, 0 failed, 0.09s
```

<sub>Captured from <code>examples/polyglot</code> in this repository — a repo spanning Node, Python and Go.</sub>

## lattice.json

One file at the root. Every command is visible, and you can run any of them by hand.

```json
{
  "latticeVersion": "0.1.0",
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

`web` and `api` are detected: Lattice reads the lockfile or declaration file already in the directory and runs that tool. `utils` opts out with `auto: false` and declares its commands outright. `^build` means "build my dependencies first."

## Features

Every task result is keyed by a hash over its command, inputs, environment and lockfiles, so changing one file rebuilds only what depends on it. The repo is one graph, and independent tasks run in parallel.

A workspace left on `auto` is resolved from the lockfile or declaration file already in its directory, and Lattice then calls that tool: npm, pnpm, cargo, go, uv, poetry, bundler, gradle, maven, dotnet and others. A subtree with its own task runner becomes one workspace whose task shells out to it.

Toolchains take a version constraint (`node`, `rust`, `go` and others). Lattice either validates the version already installed or provisions it into `./.lattice/toolchains/` and pins what it resolved.

Persistent tasks such as dev servers and watchers stream their output live and stay out of the cache.

Full documentation: **[latticeandcompany.github.io/lattice/docs](https://latticeandcompany.github.io/lattice/docs)** — [getting started](https://latticeandcompany.github.io/lattice/docs/getting-started) · [configuration](https://latticeandcompany.github.io/lattice/docs/configuration) · [caching](https://latticeandcompany.github.io/lattice/docs/caching) · [toolchains](https://latticeandcompany.github.io/lattice/docs/toolchains) · [CLI](https://latticeandcompany.github.io/lattice/docs/cli)

## Development

**Requires:** Rust 1.75+ with `rustfmt` and `clippy`. Node 26+ only for the docs site.

```bash
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build
cargo test --workspace
```

| Command | Description |
|---|---|
| `cargo build` | Debug build of every crate |
| `cargo test --workspace` | Unit, integration and end-to-end tests |
| `cargo clippy --all-targets --all-features -- -D warnings` | Lint gate |
| `scripts/stress-test.sh` | Full hermetic end-to-end suite |
| `scripts/dev-link.sh` | Point `./.lattice/bin/lattice` at your dev build |

The repo root has its own `lattice.json`, so `.lattice/bin/lattice run build` drives the crates and the docs site. See [CONTRIBUTING.md](CONTRIBUTING.md).

## Architecture

Cargo workspace:

```
crates/
  lattice/              → CLI: commands, flags, schema emission
  lattice-config/       → lattice.json types, loading, validation
  lattice-workspace/    → workspace discovery, tool detection, toolchains
  dagger/               → task dependency graph and scheduler
  lattice-cache/        → content-addressed output cache
  lattice-runner/       → async task executor
  lattice-output/       → terminal output and the interactive run UI
apps/
  web/                  → Astro documentation site
examples/
  polyglot/             → several languages, mixed detected and declared workspaces
  nested-repo/          → a subtree with its own task runner, wrapped as one workspace
```

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md) first — it covers setup, branching, the testing requirements and the AI-disclosure rule. Participation is governed by the [Code of Conduct](CODE_OF_CONDUCT.md). Vulnerabilities go through [SECURITY.md](SECURITY.md), never a public issue.

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
