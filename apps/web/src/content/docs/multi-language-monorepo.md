---
title: A multi-language monorepo
description: A worked walkthrough of examples/polyglot — four languages, one graph, one cache.
group: Guides
order: 2
---

# A multi-language monorepo

`examples/polyglot` in the Lattice repo is a small, real monorepo: a
JavaScript app, a Rust binary, a Python library, and a Go service, run and
cached from one root `lattice.json`. This page walks it end to end — what
each workspace is, what Lattice detects for it and why, how `lattice run
build` orders the four of them, and what a second run looks like once the
cache is warm.

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

Four workspaces, four languages, none of them named in the config as
anything other than a directory:

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

This is the file as shipped, unedited. Every command below runs against a
copy of this exact directory.

## What Lattice detects, workspace by workspace

`web` and `api` use the default `auto: true` and declare no `scripts` at
all — Lattice has to find their driver on its own. `utils` and `worker` set
`auto: false` and declare every command by hand. That split is not
arbitrary; each half exists for a different reason, and the evidence ladder
(see [Driver detection](/lattice/docs/drivers)) explains why.

### `web`: a lockfile picks the driver

`apps/web` has a `package.json` and a `package-lock.json`. A bare
`package.json` only says "JavaScript" — Lattice's ladder needs rung 3, a
tool-unique lockfile, to go further. `package-lock.json` is npm's, and
nothing else's, so `npm` is the driver, with the package-manager role. Every
task command comes from the `scripts` block in `apps/web/package.json`:
`npm run build`, `npm run test`, `npm run lint`.

### `api`: same ladder, a different lockfile

`apps/api` has a `Cargo.toml` and a `Cargo.lock`. `Cargo.lock` is cargo's
fingerprint (rung 3 again), so the driver is `cargo`, with
the build-tool role. Its invoke form composes the task name directly onto the
tool: `cargo build`, `cargo test`. (`cargo lint` is not a real cargo
subcommand without an alias configured for it — the invoke form is generic,
not verified against what the tool actually supports, so `lint` here would
fail if it ran for real. `build` and `test` are the ones this walkthrough
actually runs.)

### `utils`: `auto: false` even though a lockfile exists

`libs/utils` has a `uv.lock` — if this workspace were `auto: true`, the
ladder would resolve `uv` as the driver (also the package-manager role) via that
lockfile, same as `web` and `api`. It isn't: `utils` sets `auto: false` and
supplies `scripts` for all four root tasks itself, calling `python3`
directly instead of `uv run …`.

Because every task has an explicit script, `auto: false` changes nothing
observable *today* — the explicit command always wins over an inferred one,
whether or not the workspace is `auto`. What it buys is a safeguard for
tomorrow: if a task got added to `tasks` in `lattice.json` without a
matching entry in `utils`'s `scripts`, an `auto: true` workspace would
silently try to invent a `uv run <task>` command; `auto: false` makes that
same gap a hard failure instead — the workspace declares no command for
that task, and the run halts. Also worth knowing: `pyproject.toml`'s own
`requires-python = ">=3.11"` is metadata for Python tooling, not something
Lattice reads — the only Python version Lattice would ever check is one
declared under `engines`, and this config declares none.

### `worker`: `auto: false` because auto would halt

`services/worker` has a `go.mod` with no `toolchain` line and no `go.sum`
(there are no external dependencies to lock). `go.sum` is the only
fingerprint the driver registry recognizes for Go — without it, `auto: true`
finds no candidate at all. Flipping `worker` to `auto: true` in a scratch
copy of this example and running `lattice run build --dry-run` confirms it:

```text
Error: workspace 'worker' has an ambiguous or undeclared task driver.
No task driver could be detected (no lockfile, wrapper, or native declaration).
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "node": ">=0.0.0" }
```

So here `auto: false` is load-bearing, not defensive: without it (or an
explicit `engines` declaration), this workspace can't run at all. Its
`scripts` shell out to `go` directly and check `command -v go` first, so the
example still runs — printing a skip notice instead of failing — on a
machine without Go installed.

### The root `engines`, and who they apply to

`node` and `rust` are declared once, at the root, with no per-workspace
override. Every workspace's resolved engine map is the same merged map (root
`engines`, since none of the four workspaces adds its own), so Lattice
resolves it exactly once for the whole run and reuses the result for all
four — not once per workspace (see [Engines and
provisioning](/lattice/docs/engines)). Both constraints are version-only, no
`installCmd`, so this is validate-only mode: Lattice runs `node --version`
and `rustc --version` on the host and fails before any task starts if either
doesn't satisfy its constraint. Nothing is installed. This validation runs
for the whole graph regardless of which workspace actually uses Node or
Rust — `utils` and `worker` are checked against the same `node`/`rust`
constraints as `web` and `api`, even though neither runs a line of
JavaScript or Rust.

## Running `build`

`lattice run build --dry-run` shows the graph without running anything:

```text
❖ lattice  dry run · build
  → worker:build  sh -c 'if command -v go >/dev/null; then go build ./...; else echo "[worker] go not installed, skipping native build"; fi'
  → utils:build  python3 -c "import os; os.makedirs('dist', exist_ok=True); import shutil; shutil.copy('src/utils.py', 'dist/utils.py'); print('utils build complete')"
  → api:build  cargo build
  → web:build  npm run build
```

Four nodes, no edges between them. `build`'s task-level `dependsOn: ["^build"]`
means "build every workspace this one depends on, first" — but none of the
four workspaces here declares a `dependsOn` of its own, so `^build` has
nothing to expand into for any of them. All four are graph roots. Running it
for real (`-l` for plain line-by-line output instead of the interactive TUI)
shows exactly that: every build starts at once and finishes independently,
bounded only by the concurrency limit (default: logical CPUs — see
[Selecting what runs](/lattice/docs/filtering)):

```text
lattice: running `build` across 4 workspace(s)
lattice: worker:build: hash ec52d859e992ff10
lattice: worker:build: cache miss
worker:build: running
lattice: web:build: hash 839dbe990f662989
lattice: utils:build: hash 2328e123c2be5737
lattice: utils:build: cache miss
utils:build: running
lattice: web:build: cache miss
web:build: running
lattice: api:build: hash ca9cf431598d41a5
lattice: api:build: cache miss
api:build: running
worker:build: [worker] go not installed, skipping native build
worker:build: done (0.01s)
utils:build: utils build complete
utils:build: done (0.08s)
web:build: Building web app...
web:build: done (0.26s)
api:build:    Compiling api v0.1.0 (…/apps/api)
api:build:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.67s
api:build: done (0.71s)
lattice: 4 tasks, 0 cached, 0 failed, 1.05s
```

That output is real — captured from `cargo build -p lattice`'s binary run
against a copy of this example, on a machine with Node and Rust installed
and Go absent, which is exactly what the `worker` script's own
`command -v go` check is there to handle. Every task shows a cache miss
first: nothing has ever built here before, so there's nothing to restore
(see [Caching](/lattice/docs/caching) for what feeds that hash).

## Running it again: the cache

Run `lattice run build -l` a second time with nothing changed:

```text
lattice: running `build` across 4 workspace(s)
lattice: worker:build: hash ec52d859e992ff10
lattice: web:build: hash 839dbe990f662989
lattice: api:build: hash ca9cf431598d41a5
lattice: utils:build: hash 2328e123c2be5737
worker:build: cache hit [ec52d859]
web:build: cache hit [839dbe99]
utils:build: cache hit [2328e123]
api:build: cache hit [ca9cf431]
lattice: 4 tasks, 4 cached, 0 failed, 0.07s
```

Same four hashes as the first run — `build`'s `inputs` is `src/**/*`, and no
source file changed, so every workspace's key is identical and every lookup
is a hit. 1.05s of real compiling and copying became 0.07s of restoring from
`.lattice/cache`. Edit one workspace's source and only that workspace's key
changes: appending a line to `apps/web/src/index.js` and re-running `build`
reruns `web:build` alone —

```text
lattice: web:build: cache miss
web:build: running
worker:build: cache hit [ec52d859]
utils:build: cache hit [2328e123]
api:build: cache hit [ca9cf431]
web:build: Building web app...
web:build: done (0.22s)
lattice: 4 tasks, 3 cached, 0 failed, 0.28s
```

— while `worker`, `utils`, and `api` come back as hits, because `build`'s
`inputs` (`src/**/*`) never matched anything under `apps/web` for them in
the first place.

## The cross-language dependency edge

As shipped, `examples/polyglot`'s four workspaces build independently — the
previous section's dry run has no edges to show. But the mechanism for a
real cross-language dependency is exactly the same file, one field: a
workspace's `dependsOn`. Say `worker` (Go) actually needed `utils` (Python)
built first — for instance, its build step shells out to something `utils`
produces. You'd declare that on the `worker` workspace, and only there:

```json
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
```

Nothing else in the file changes: `build`'s task-level `dependsOn: ["^build"]`
already means "build every workspace I depend on first," for every
workspace that has that dependency. Adding `dependsOn: ["utils"]` to
`worker` is what gives `^build` something to resolve to for `worker`
specifically. `lattice run build --dry-run` against that one-line change
shows the new edge immediately:

```text
❖ lattice  dry run · build
  → utils:build  python3 -c "import os; os.makedirs('dist', exist_ok=True); import shutil; shutil.copy('src/utils.py', 'dist/utils.py'); print('utils build complete')"
  → worker:build  sh -c 'if command -v go >/dev/null; then go build ./...; else echo "[worker] go not installed, skipping native build"; fi'
  → api:build  cargo build
  → web:build  npm run build
```

`utils:build` now precedes `worker:build`; `api` and `web` are still
independent roots. Running it for real shows the same ordering play out:
`api:build` and `web:build` start immediately, `utils:build` starts
immediately too, and `worker:build`'s hash and start only appear once
`utils:build` finishes —

```text
lattice: utils:build: hash 2328e123c2be5737
lattice: utils:build: cache miss
utils:build: running
lattice: api:build: hash ca9cf431598d41a5
lattice: web:build: hash ea603568d0b9ef0e
lattice: web:build: cache miss
web:build: running
lattice: api:build: cache miss
api:build: running
api:build:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.00s
utils:build: utils build complete
utils:build: done (0.03s)
lattice: worker:build: hash ec52d859e992ff10
lattice: worker:build: cache miss
worker:build: running
worker:build: [worker] go not installed, skipping native build
worker:build: done (0.01s)
```

— confirmed by adding that one field to a scratch copy of this example and
running both commands against it. Lattice does not care that the dependency
crosses a language boundary: the edge between `worker:build` and
`utils:build` is built the same way it would be if both workspaces were
Go, or both were Python — from `dependsOn` naming a workspace, not a
language or a driver. The full mechanics of `dependsOn`, the `task` vs.
`^task` distinction, and how several requested tasks merge into one graph
are covered in [Task graph](/lattice/docs/task-graph).
