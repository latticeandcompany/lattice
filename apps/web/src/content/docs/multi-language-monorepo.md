---
title: Run tasks across languages in one repo
description: One root config that builds and caches JavaScript, Rust, Python, and Go together.
group: Guides
order: 2
---

# Run tasks across languages in one repo

One `lattice.json` at the repo root declares every workspace and every task, and
nothing in it names a language. This guide works through
`examples/polyglot` in the Lattice repo: a JavaScript app, a Rust binary, a
Python library, and a Go service, all built and cached from that one file. Every
command and every block below was captured against a copy of it.

## The repo

```text
examples/polyglot/
  lattice.json
  apps/
    web/      package.json, package-lock.json (an Express app)
    api/      Cargo.toml, Cargo.lock (a Rust binary)
  libs/
    utils/    pyproject.toml, uv.lock (a Python library)
  services/
    worker/   go.mod (a Go binary)
```

The config as shipped:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-3",
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

## Let detection cover what it can

Two of the four workspaces declare no commands at all. Lattice finds their
driver from a tool-unique file already in the directory.

| Workspace | Evidence | Driver | Commands |
| --- | --- | --- | --- |
| `web` | `package-lock.json` | `npm` | `npm run build`, `npm run test`, `npm run lint` |
| `api` | `Cargo.lock` | `cargo` | `cargo build`, `cargo test` |
| `utils` | declared | none, `auto: false` | its own `scripts` |
| `worker` | declared | none, `auto: false` | its own `scripts` |

A bare `package.json` would only say "JavaScript". `package-lock.json` says
`npm` and nothing else, which is what makes the inference safe. See [Driver
detection](/lattice/docs/drivers).

`cargo`'s inferred form puts the task name straight onto the tool, so `lint`
would resolve to `cargo lint` and fail unless the workspace has an alias for it.
`api` runs `build` and `test` only.

## Declare the commands detection cannot infer

`utils` and `worker` set `"auto": false` and write every command out, for two
different reasons.

`libs/utils` has a `uv.lock`, so detection would resolve `uv` and produce
`uv run build`. The example wants `python3` directly, so it declares the four
commands instead. Lattice reads no Python version from `pyproject.toml`. The
only version it checks is one declared under `engines`.

`services/worker` has a `go.mod` with no `toolchain` line and no `go.sum`,
because it has no external dependencies to lock. `go.sum` is the only
fingerprint the driver table recognizes for Go, so at `auto: true` there is no
candidate and the run halts:

```text
Error: workspace 'worker' has an ambiguous or undeclared driver.
Lattice detected no driver. The directory holds no lockfile, no wrapper, and no native declaration.
Declare the driver in lattice.json, under this workspace:
  "engines": { "node": ">=0.0.0" }
```

`auto: false` plus `scripts` is what makes that workspace runnable. Its scripts
check `command -v go` first, so a machine without Go prints a skip notice
instead of failing.

## Order work across a language boundary

Two fields together put `utils` before `worker`:

```json
{
  "name": "worker",
  "path": "services/worker",
  "auto": false,
  "dependsOn": ["utils"],
  "scripts": { }
}
```

A workspace's `dependsOn` states what `worker` depends on and orders nothing by
itself. A task's `dependsOn` states what runs first, and `build`'s is
`["^build"]`, meaning the `build` task of every workspace this one depends on.
The `^` is what reads the workspace edge. Drop either half and the graph
flattens.

The edge names a workspace, not a language or a driver, so it works the same
across a language boundary as it would between two Go workspaces. `worker`'s
`go build` waits on `utils`'s `python3`, and neither driver knows the other
exists. See [Task graph](/lattice/docs/task-graph).

## Run it

`--dry-run` prints the resolved graph and runs nothing:

```text
❖ lattice  dry run · build
  → utils:build  python3 -c "import os; os.makedirs('dist', exist_ok=True); import shutil; shutil.copy('src/utils.py', 'dist/utils.py'); print('utils build complete')"
  → worker:build  sh -c 'if command -v go >/dev/null; then go build ./...; else echo "[worker] go not installed, skipping native build"; fi'
  → api:build  cargo build
  → web:build  npm run build
```

Four nodes, one edge. `utils` is the only workspace anything depends on, so
`^build` expands to `utils:build` for `worker` and to nothing for the other
three.

Now run it for real. `-v` gives the plain line-by-line log instead of the live
display:

```text
$ lattice run build -v
lattice: running `build` across 4 workspaces
lattice: api:build: hash b00c22c209fa3dbc
lattice: web:build: hash 19ac4ce220d5c90a
lattice: api:build: cache miss (nothing cached for this task yet)
lattice: web:build: cache miss (nothing cached for this task yet)
api:build: running
web:build: running
lattice: utils:build: hash 416602fcd8c04e1b
lattice: utils:build: cache miss (nothing cached for this task yet)
utils:build: running
api:build:    Compiling api v0.1.0 (…/apps/api)
utils:build: utils build complete
utils:build: done (0.04s)
lattice: worker:build: hash 8f30909a68ff83e6
lattice: worker:build: cache miss (nothing cached for this task yet)
worker:build: running
worker:build: [worker] go not installed, skipping native build
lattice: warning: worker:build: failed to cache outputs: no files matched outputs ["dist/**", "target/debug/**", "bin/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
worker:build: done (0.01s)
api:build:     Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.13s
web:build: Building web app...
web:build: done (0.26s)
api:build: done (0.15s)
lattice: 4 tasks, 0 cached, 0 failed, 0.67s
```

`utils`, `api`, and `web` all start at once, bounded by the concurrency limit,
which defaults to the number of logical CPUs. `worker` is not hashed or started
until `utils:build` finishes.

The warning on `worker:build` is worth reading. The capture machine has Node and
Rust but no Go, so the `command -v go` check took the skip branch and the task
wrote nothing. Lattice warns rather than store an empty entry. With Go
installed the task would build, but `go build ./...` writes its binary to the
workspace directory rather than to `bin/`, so nothing would match the shared
`outputs` patterns then either. `worker:build` runs every time in this example
and is the reason a second run reports three of four cached instead of four.

## Run it again

With nothing changed, the three workspaces that produced output come back from
the cache:

```text
$ lattice run build -v
lattice: running `build` across 4 workspaces
lattice: web:build: hash 19ac4ce220d5c90a
lattice: utils:build: hash 416602fcd8c04e1b
lattice: api:build: hash b00c22c209fa3dbc
web:build: cache hit [19ac4ce2]
utils:build: cache hit [416602fc]
lattice: worker:build: hash 8f30909a68ff83e6
lattice: worker:build: cache miss (the entry for this key is no longer in the cache)
worker:build: running
worker:build: [worker] go not installed, skipping native build
lattice: warning: worker:build: failed to cache outputs: no files matched outputs ["dist/**", "target/debug/**", "bin/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
worker:build: done (0.01s)
api:build: cache hit [b00c22c2]
lattice: 4 tasks, 3 cached, 0 failed, 0.10s, 0.45s saved
```

Same hashes as the first run. `build`'s `inputs` is `src/**/*`, no source file
changed, so every key is identical.

Edit one workspace's source and only that workspace misses. Appending a line to
`libs/utils/src/utils.py` reruns `utils:build`, and `worker:build` reruns behind
it because a task's key includes the keys of its prerequisites:

```text
$ lattice run build -v
lattice: running `build` across 4 workspaces
lattice: api:build: hash b00c22c209fa3dbc
lattice: utils:build: hash 51e3891610471da7
lattice: web:build: hash 19ac4ce220d5c90a
lattice: utils:build: cache miss: inputs changed
utils:build: running
web:build: cache hit [19ac4ce2]
utils:build: utils build complete
utils:build: done (0.03s)
lattice: worker:build: hash a0ac64a07eaaf728
lattice: worker:build: cache miss: dependencies changed
worker:build: running
worker:build: [worker] go not installed, skipping native build
lattice: warning: worker:build: failed to cache outputs: no files matched outputs ["dist/**", "target/debug/**", "bin/**"], so nothing was cached. Check that the patterns are relative to the workspace, and that the task writes there
worker:build: done (0.01s)
api:build: cache hit [b00c22c2]
lattice: 4 tasks, 2 cached, 0 failed, 0.11s, 0.41s saved
```

`web` and `api` are untouched: `build`'s `inputs` resolves per workspace and
never matched anything under `libs/utils` for them. `dependencies changed` is
the miss reason that names an upstream cause. See
[Caching](/lattice/docs/caching).

## Validate one toolchain for the whole repo

`node` and `rust` are declared once, at the root, and no workspace overrides
them. Both are version-only with no `installCmd`, so Lattice runs
`node --version` and `rustc --version` on the host and fails before any task
starts if either constraint is unsatisfied. Nothing is installed.

Validation covers the whole graph, not the workspaces that use those tools:
`utils` and `worker` are checked against the same constraints as `web` and
`api`. To install a pinned copy instead of trusting the host, add an
`installCmd`. See [Pinning tool
versions](/lattice/docs/pinning-tool-versions).

## Next

- [Task graph](/lattice/docs/task-graph) for `^task` versus `task`, and how
  several requested tasks merge into one graph.
- [Driver detection](/lattice/docs/drivers) for the full evidence ladder and the
  built-in driver table.
- [Nested repos](/lattice/docs/nested-repos) for a workspace that has its own
  task runner underneath.
