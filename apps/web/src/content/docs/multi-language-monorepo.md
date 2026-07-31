---
title: A multi-language monorepo
description: A worked walkthrough of a repo spanning Node, Python, Go and Rust in one task graph.
group: Guides
order: 2
---

# A multi-language monorepo

`examples/polyglot` in the Lattice repo is a small, working monorepo: a
JavaScript app, a Rust binary, a Python library, and a Go service, run and
cached from one root `lattice.json`.

## The repo

```text
examples/polyglot/
  lattice.json
  apps/
    web/      package.json, package-lock.json — an Express app
    api/      Cargo.toml, Cargo.lock — a Rust binary
  libs/
    utils/    pyproject.toml, uv.lock — a Python library
  services/
    worker/   go.mod — a Go binary
```

The config declares each of the four as a directory and a name:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    { "name": "web", "path": "apps/web" },
    { "name": "api", "path": "apps/api" },
    {
      "name": "utils",
      "path": "libs/utils",
      "auto": false,
      "scripts": {
        "build": "python3 -c \"import os; os.makedirs('dist', exist_ok=True); import shutil; shutil.copy('src/utils.py', 'dist/utils.py'); print('utils build complete')\"",
        "test": "python3 src/utils.py",
        "lint": "python3 -m py_compile src/utils.py && echo 'Syntax OK'",
        "clean": "python3 -c \"import shutil; shutil.rmtree('dist', ignore_errors=True); print('utils clean complete')\""
      }
    },
    {
      "name": "worker",
      "path": "services/worker",
      "auto": false,
      "dependsOn": ["utils"],
      "scripts": {
        "build": "sh -c 'if command -v go >/dev/null; then go build ./...; else echo \"[worker] go not installed, skipping native build\"; fi'",
        "test": "sh -c 'if command -v go >/dev/null; then go test ./...; else echo \"[worker] go not installed, skipping tests\"; fi'",
        "lint": "sh -c 'if command -v go >/dev/null; then go vet ./...; else echo \"[worker] go not installed, skipping lint\"; fi'",
        "clean": "sh -c 'rm -rf bin && echo worker clean complete'"
      }
    }
  ],
  "engines": {
    "node": ">=20.0.0",
    "rust": ">=1.75.0"
  },
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*"],
      "ignore": ["**/*.test.*", "**/README.md"],
      "outputs": ["dist/**", "target/debug/**", "bin/**"]
    },
    "test": {
      "dependsOn": ["build"],
      "inputs": ["src/**/*", "tests/**/*"]
    },
    "lint": {
      "inputs": ["src/**/*"]
    },
    "clean": {}
  },
  "settings": {
    "maxCacheSize": "1GB"
  }
}
```

That is the file as shipped. Every command below runs against a copy of this
directory.

## What Lattice detects, workspace by workspace

`web` and `api` use the default `auto: true` and declare no `scripts`, so
Lattice finds their driver from evidence in the directory. `utils` and `worker`
set `auto: false` and declare every command by hand, for two different reasons.

### `web`: npm, from `package-lock.json`

`apps/web` has a `package.json` and a `package-lock.json`. A bare `package.json`
only says "JavaScript", so the ladder needs rung 3, a tool-unique lockfile.
`package-lock.json` is npm's and nothing else's, so `npm` is the driver, with
the package-manager role. Every task command comes from the `scripts` block in
`apps/web/package.json`: `npm run build`, `npm run test`, `npm run lint`.

### `api`: cargo, from `Cargo.lock`

`apps/api` has a `Cargo.toml` and a `Cargo.lock`. `Cargo.lock` is cargo's
fingerprint (rung 3 again), so the driver is `cargo`, with the build-tool role.
Its invoke form composes the task name onto the tool: `cargo build`,
`cargo test`. That form is generic and not checked against what the tool
supports, so `lint` would fail here — `cargo lint` is not a subcommand without
an alias configured for it. `build` and `test` are the tasks this walkthrough
runs.

### `utils`: declared `scripts` over an existing lockfile

`libs/utils` has a `uv.lock`, so at `auto: true` the ladder would resolve `uv`
as the driver (also the package-manager role). Instead `utils` sets
`auto: false` and supplies `scripts` for all four root tasks, calling `python3`
directly rather than `uv run …`.

Since every task has an explicit script, `auto: false` changes nothing
observable today: an explicit command wins over an inferred one whether or not
the workspace is `auto`. It matters for the next task added to `tasks` without a
matching `scripts` entry. An `auto: true` workspace would invent a
`uv run <task>` command for it; `auto: false` makes it a hard failure, because
the workspace declares no command for that task and the run halts.

`pyproject.toml`'s own `requires-python = ">=3.11"` is metadata for Python
tooling and Lattice does not read it. The only Python version Lattice checks is
one declared under `engines`, and this config declares none.

### `worker`: no detectable driver

`services/worker` has a `go.mod` with no `toolchain` line and no `go.sum` (there
are no external dependencies to lock). `go.sum` is the only fingerprint the
driver registry recognizes for Go, so at `auto: true` there is no candidate at
all. Flipping `worker` to `auto: true` in a scratch copy and running
`lattice run build --dry-run` shows it:

```text
Error: workspace 'worker' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "node": ">=0.0.0" }
```

Without `auto: false`, or an explicit `engines` declaration, this workspace
can't run. Its `scripts` shell out to `go` directly and check `command -v go`
first, so on a machine without Go the example prints a skip notice instead of
failing.

### Root `engines`

`node` and `rust` are declared once, at the root, with no per-workspace
override. None of the four workspaces adds its own, so every workspace resolves
the same merged engine map and Lattice resolves it once for the whole run rather
than once per workspace (see [Engines and provisioning](/lattice/docs/engines)).
Both constraints are version-only with no `installCmd`, which is validate-only
mode: Lattice runs `node --version` and `rustc --version` on the host and fails
before any task starts if either doesn't satisfy its constraint. Nothing is
installed.

Validation covers the whole graph regardless of which workspace uses Node or
Rust. `utils` and `worker` are checked against the same constraints as `web` and
`api`, even though neither runs a line of JavaScript or Rust.

## Running `build`

`lattice run build --dry-run` shows the graph without running anything:

```text
❖ lattice  dry run · build
  → utils:build  python3 -c "import os; os.makedirs('dist', exist_ok=True); import shutil; shutil.copy('src/utils.py', 'dist/utils.py'); print('utils build complete')"
  → worker:build  sh -c 'if command -v go >/dev/null; then go build ./...; else echo "[worker] go not installed, skipping native build"; fi'
  → api:build  cargo build
  → web:build  npm run build
```

Four nodes and one edge. `build`'s task-level `dependsOn: ["^build"]` means
"build every workspace this one depends on, first," and `worker` is the only
workspace declaring a `dependsOn` of its own (`["utils"]`), so `^build` expands
to `utils:build` for `worker` and to nothing for the other three. `utils:build`
precedes `worker:build`; `api` and `web` are independent roots.

Running it for real (`-l` for plain line-by-line output instead of the
interactive TUI) shows the ordering. `utils`, `api`, and `web` all start at
once, bounded by the concurrency limit (default: logical CPUs — see [Selecting
what runs](/lattice/docs/filtering)); `worker` isn't hashed or started until
`utils:build` finishes:

```text
lattice: running `build` across 4 workspace(s)
lattice: api:build: hash ca9cf431598d41a5
lattice: web:build: hash 839dbe990f662989
lattice: api:build: cache miss
api:build: running
lattice: utils:build: hash 2328e123c2be5737
lattice: web:build: cache miss
web:build: running
lattice: utils:build: cache miss
utils:build: running
utils:build: utils build complete
utils:build: done (0.14s)
lattice: worker:build: hash ec52d859e992ff10
lattice: worker:build: cache miss
worker:build: running
worker:build: [worker] go not installed, skipping native build
worker:build: done (0.01s)
api:build:    Compiling api v0.1.0 (…/apps/api)
web:build: Building web app...
web:build: done (0.63s)
api:build:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.79s
api:build: done (0.83s)
lattice: 4 tasks, 0 cached, 0 failed, 1.09s
```

That output was captured against a copy of this example on a machine with Node
and Rust installed and Go absent, the case the `worker` script's
`command -v go` check handles. Every task misses because nothing has ever built
here before (see [Caching](/lattice/docs/caching) for what feeds the hash).

## Running it again: the cache

Run `lattice run build -l` a second time with nothing changed:

```text
lattice: running `build` across 4 workspace(s)
lattice: utils:build: hash 2328e123c2be5737
lattice: api:build: hash ca9cf431598d41a5
lattice: web:build: hash 839dbe990f662989
web:build: cache hit [839dbe99]
utils:build: cache hit [2328e123]
lattice: worker:build: hash ec52d859e992ff10
worker:build: cache hit [ec52d859]
api:build: cache hit [ca9cf431]
lattice: 4 tasks, 4 cached, 0 failed, 0.12s
lattice: full cache — nothing to run
```

Same four hashes as the first run. `build`'s `inputs` is `src/**/*`, no source
file changed, so every key is identical and every lookup restores from
`.lattice/cache` instead of running. Edit one workspace's source and only that
workspace's key changes; appending a line to `apps/web/src/index.js` and
re-running `build` reruns `web:build` alone —

```text
lattice: running `build` across 4 workspace(s)
lattice: api:build: hash ca9cf431598d41a5
lattice: web:build: hash 9f15bc51cad38b32
lattice: utils:build: hash 2328e123c2be5737
lattice: web:build: cache miss
web:build: running
utils:build: cache hit [2328e123]
lattice: worker:build: hash ec52d859e992ff10
worker:build: cache hit [ec52d859]
api:build: cache hit [ca9cf431]
web:build: Building web app...
web:build: done (0.32s)
lattice: 4 tasks, 3 cached, 0 failed, 0.39s
```

— while `worker`, `utils`, and `api` come back as hits, because `build`'s
`inputs` (`src/**/*`) resolves per workspace and never matched anything under
`apps/web` for them.

## The cross-language dependency edge

The ordering between `utils` and `worker` comes from one field on one
workspace:

```json
{
  "name": "worker",
  "path": "services/worker",
  "auto": false,
  "dependsOn": ["utils"],
  "scripts": { … }
}
```

Both halves are required. A workspace's `dependsOn` states what `worker` depends
on and orders nothing by itself. A task's `dependsOn` states what runs first,
and `build`'s is `["^build"]`, "the `build` task of each workspace this one
depends on"; the `^` is what reads the workspace edge. Drop either half and the
graph flattens: without `worker`'s `dependsOn`, `^build` expands to nothing, and
with a task-level `build` that has no `^build`, nothing consults the workspace
edge.

The edge is built from `dependsOn` naming a workspace, not a language or a
driver, so it works the same across a language boundary as it would between two
Go workspaces. `worker`'s `go build` waits on `utils`'s `python3`, and neither
driver knows the other exists.

The edge does not change `worker`'s cache key. Edit `libs/utils/src/utils.py`
and re-run: `utils:build` misses and reruns, while `worker:build` still hashes
to `ec52d859e992ff10` and comes back a hit, because a task's key covers its own
`inputs`, command, `env`, and toolchain and never its dependencies' (see
[Caching](/lattice/docs/caching)). `dependsOn` orders `utils:build` first; it
does not make `worker:build` stale when `utils` changes.

The full mechanics of `dependsOn`, the `task` vs. `^task` distinction, and
how several requested tasks merge into one graph are covered in
[Task graph](/lattice/docs/task-graph).
