---
title: Run tasks across languages in one repo
description: One config that builds and caches JavaScript, Rust, Python, and Go together.
group: Guides
order: 2
---

# Run tasks across languages in one repo

Nothing in Lattice's model is specific to a language. A workspace is a
directory, its tool comes from its files, and an edge between two workspaces
works the same whether they share a language or not. This guide works through
`examples/polyglot` in the Lattice repo: a JavaScript app, a Rust binary, a
Python library, and a Go service.

## The repo

```text
examples/polyglot/
  lattice.json
  apps/
    web/      package.json, package-lock.json
    api/      Cargo.toml, Cargo.lock
  libs/
    utils/    pyproject.toml, uv.lock
  services/
    worker/   go.mod
```

No workspace file at the root names these four, because no single tool owns
them. The config lists the directories, and says the three things the manifests
cannot:

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.1.1",
  "workspaces": ["apps/*", "libs/*", "services/*"],
  "tasks": {
    "build": { "ignore": ["**/*.test.*", "**/README.md"] },
    "test": { "dependsOn": ["build"] }
  },
  "overrides": {
    "web": { "engines": { "node": ">=20.0.0" } },
    "api": { "engines": { "rust": ">=1.75.0" } },
    "utils": {
      "tasks": {
        "build": {
          "run": "python3 -c \"import os, shutil; os.makedirs('dist', exist_ok=True); shutil.copy('src/utils.py', 'dist/utils.py')\"",
          "outputs": ["dist/**"]
        },
        "test": "python3 src/utils.py",
        "lint": "python3 -m py_compile src/utils.py"
      }
    },
    "worker": { "dependsOn": ["utils"] }
  },
  "settings": { "maxCacheSize": "1GB" }
}
```

## What Lattice works out on its own

| Workspace | Evidence | Tool | Tasks |
| --- | --- | --- | --- |
| `web` | `package-lock.json` | `npm` | every `package.json` script: `build`, `test`, `lint`, `dev`, `clean` |
| `api` | `Cargo.lock` | `cargo` | `build`, `test`, `check`, `lint` (`cargo clippy`), `format`, `doc`, `bench`, `clean` |
| `utils` | `uv.lock` | `uv` | `build` (`uv build`), plus the three written out |
| `worker` | `go.mod` | `go` | `build` (`go build ./...`), `test`, `check`, `lint` (`go vet ./...`), `format`, `clean` |

Each name comes from its manifest, or the directory for Go. A bare `go.mod` is
enough to pick Go: no other tool reads it. See [Driver
detection](/lattice/docs/drivers).

## Write out what detection would get wrong

`utils` has a `uv.lock`, so its `build` would be `uv build`, which produces a
wheel. The example wants a plain copy into `dist/`, so the override writes the
command, with the `outputs` it produces. `test` and `lint` are added the same
way; a uv project gets a `test` verb only when `pytest` is a dependency, and this
one has none.

Written-out commands and discovered ones mix freely. `utils` keeps every other
task `uv` gives it.

## Order work across a language boundary

`worker` is a Go service that reads a file the Python library builds. No
manifest can say that, so the override does:

```json
{ "overrides": { "worker": { "dependsOn": ["utils"] } } }
```

`build` depends on `^build` by default, so `worker:build` now waits for
`utils:build`. The edge names a workspace, not a language or a tool, and neither
`go` nor `python3` knows the other exists. Edges within one language — a pnpm
package depending on another, a crate on a crate — come from the manifests and
need no entry. See [Task graph](/lattice/docs/task-graph).

## Run it

```text
$ lattice run build --dry-run
❖ lattice  dry run · build
  → utils:build  python3 -c "import os, shutil; os.makedirs('dist', exist_ok=True); shutil.copy('src/utils.py', 'dist/utils.py')"
  → worker:build  go build ./...
  → web:build  npm run build
  → api:build  cargo build
```

`utils`, `web` and `api` start together, bounded by the concurrency limit;
`worker` starts when `utils` finishes. On a terminal, the live display shows the
three at once with the last lines each printed, and `worker` waiting on `utils`.

A second run with nothing changed comes back from cache, except where a task
stored nothing. `web` caches `dist/**` by default and `utils` declares its own;
`api`'s Cargo build writes to a `target/` directory Lattice does not claim by
default, and `go build ./...` writes nothing it keeps, so give either task
`outputs` if its artifact should be restored.

Edit `libs/utils/src/utils.py` and run again: `utils:build` misses on `inputs`,
and `worker:build` misses on `dependencies`, because a task's key includes its
prerequisites' keys. `web` and `api` stay cached. `lattice run build --stream -v`
prints the reason for each miss. See [Caching](/lattice/docs/caching).

`lattice run lint` runs `npm run lint` in `web`, `cargo clippy` in `api`,
`python3 -m py_compile ...` in `utils` and `go vet ./...` in `worker`: one name,
each workspace's own meaning.

## Pin a tool for the workspaces that use it

`node` is pinned for `web` and `rust` for `api`, each in its own override, so a
machine that only works on the Python library is never asked for a Rust
compiler. Both are version checks with no `installCmd`: Lattice runs
`node --version` and `rustc --version` on the host and stops before any task
starts if either is wrong. A pin at the root would apply to every workspace
instead. See [Pinning tool versions](/lattice/docs/pinning-tool-versions).

## Next

- [Task graph](/lattice/docs/task-graph) for `^task` versus `task`.
- [Driver detection](/lattice/docs/drivers) for what each tool can run.
- [Nested repos](/lattice/docs/nested-repos) for a workspace with its own task
  runner.
