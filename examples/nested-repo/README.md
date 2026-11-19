# Nested repo

This example's frontend is its own monorepo with its own task runner, wired into
the Lattice graph as a single workspace. Lattice schedules and caches the
`frontend` workspace as one unit. The runner inside `frontend` still resolves the
packages under it.

```text
lattice.json          two workspaces: frontend, api
frontend/             an inner monorepo with its own turbo.json and package tree
  packages/ui/        built first (inner dependency)
  packages/site/      depends on ui, emits dist/bundle.js
services/api/         a plain directory with a shell script, serving site's bundle
```

`lattice.json` names `frontend` as one workspace. Lattice sees its `turbo.json`,
runs each task as `turbo run <task>` there, and finds `turbo` in the inner repo's
own `node_modules`. Nothing in `lattice.json` names the inner packages.

`services/api` holds no manifest at all, so its override writes each command out
and gives it a name.

## Run it

The inner repo needs its dependencies once:

```sh
cd frontend && npm install && cd ..
```

Then, from this directory:

```sh
lattice run build          # frontend (via the inner runner), then api
lattice run build          # both come back from cache
lattice run test
lattice run build --filter frontend
lattice run serve          # api reads the bundle frontend produced
lattice run clean
```

`frontend` has no `serve` task, so it sits that run out. `serve` depends on
`build`, so `frontend:build` runs first, from cache if the bundle is current.

## Behavior

`api` declares `"dependsOn": ["frontend"]`, so `frontend:build` completes before
`api:build` starts. Lattice never sees the `ui` to `site` edge. The inner runner
resolves that inside the workspace.

`frontend` is cached as one unit. A cache hit restores every package's `dist/` in
one step and skips the inner runner entirely. A miss hands the whole workspace
back to the inner runner, which applies its own caching per package.

With no `inputs`, every file in the workspace that git does not ignore is hashed,
so the `ignore` set keeps the inner repo's install and cache out of the key:

- `**/node_modules/**`, the installed tree. `package-lock.json` is hashed
  instead.
- `**/.turbo/**`, the inner runner's own cache, which changes on every run.

## Cache keys reach across the graph

A task's cache key includes the resolved keys of the tasks it depends on. So a
frontend-only edit changes `api:build`'s key as well as `frontend:build`'s, and
`api:build` runs again instead of restoring a result built against code that no
longer exists.

The rule is deliberately coarse: a workspace is one node, so `api:build` re-runs
whenever `frontend:build`'s key moves, even when nothing `api` reads actually
changed.

See [Nested repos](https://latticeandcompany.github.io/lattice/docs/nested-repos)
for the pattern in full.
