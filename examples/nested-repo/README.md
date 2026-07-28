# Nested repo

A repo whose frontend is its own monorepo with its own task runner, wired into the
Lattice graph as a single workspace. Lattice schedules and caches that workspace,
and the runner inside it still resolves the packages beneath it.

```
lattice.json          two workspaces: frontend, api
frontend/             an inner monorepo — turbo.json, npm workspaces
  packages/ui/        built first (inner dependency)
  packages/site/      depends on ui, emits dist/bundle.js
services/api/         a plain workspace; serves site's bundle
```

`frontend` is a manual workspace (`"auto": false`) whose scripts shell out to
`node_modules/.bin/turbo run <task>`. Nothing in `lattice.json` mentions the inner
packages, and the repo declares no `engines`: the runner comes from the inner
repo's own `node_modules`.

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
lattice run serve --filter api    # api reads the bundle frontend produced
lattice run clean
```

`serve` is filtered to `api` because a manual workspace must declare a script for
any task you invoke directly, and `frontend` has nothing to serve. Tasks that are
only pulled in as dependencies are skipped where they don't apply.

## Behavior

`api` declares `"dependsOn": ["frontend"]`, so `frontend:build` completes before
`api:build` starts. Lattice never sees the `ui → site` edge; the inner runner
resolves that inside the workspace.

`frontend` is cached as one unit. A hit restores every package's `dist/` in one
step and skips the inner runner entirely. A miss hands the whole workspace back to
it, and it applies its own caching per package.

`build` uses broad `inputs` (`**/*`), because the nested repo owns its own layout,
so the `ignore` set is what keeps the key correct. Three things have to stay out of
it:

- `**/node_modules/**` — the installed tree (`package-lock.json` is hashed instead)
- `**/.turbo/**` — the inner runner's own cache, which changes on every run
- `**/dist/**` — outputs; an output inside `inputs` invalidates the key that
  produced it

## Caveat: don't copy an upstream artifact at build time

A workspace's cache key covers its own declared inputs. It does not include the
keys of the workspaces it depends on, and `inputs` globs cannot reach outside the
workspace directory. So if `api:build` copied `frontend`'s bundle into its own
`dist/`, then a frontend-only edit would rebuild `frontend` and still serve `api`
a cache hit — leaving the copy stale.

That is why `api` builds only from its own `src/`, and reads the frontend bundle
at run time in the uncached `serve` task.

See [Nested repos](https://latticeandcompany.github.io/lattice/docs/nested-repos) for the pattern in
full.
