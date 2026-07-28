---
title: Nested repos
description: Wrapping a repo that already has its own task runner.
group: Guides
order: 2
---

# Nested repos

Some subtrees already have a task runner of their own: a JS monorepo with its own
task config, a JVM sub-repo, a directory driven by a Makefile. Lattice wraps one as a
manual workspace whose scripts shell out to that runner. It becomes a single node in
the graph, ordered against everything else, run once, and cached as one unit.

This is the ordinary manual-workspace mechanism. You declare the scripts, so Lattice
does no detection here and the workspace needs no `engines`.

```json
{
  "workspaces": [
    {
      "name": "frontend",
      "path": "frontend",
      "auto": false,
      "scripts": {
        "build": "turbo run build",
        "test": "turbo run test"
      }
    },
    { "name": "api", "path": "services/api", "dependsOn": ["frontend"] }
  ],
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["**/*"],
      "ignore": ["**/node_modules/**", "**/.turbo/**", "**/dist/**"],
      "outputs": ["dist/**", "packages/*/dist/**"]
    }
  }
}
```

`"auto": false` tells Lattice not to infer a command from the directory's manifests,
because you are supplying the commands yourself. The runner is resolved off `PATH`,
the same as any other tool the repo already expects.

A worked example lives in the repository at
[`examples/nested-repo`](https://github.com/latticeandcompany/lattice/tree/mega/examples/nested-repo).

## What Lattice owns, and what it doesn't

Lattice sees one workspace. It orders `frontend:build` against the rest of the
graph, runs it once, and caches its result. The inner runner keeps owning
everything inside: which packages exist, what depends on what, and its own
per-package cache.

The two graphs stay separate. Lattice does not interleave the inner graph with the
outer one, so an inner package can't start before an unrelated outer workspace
finishes. What you get back is a small outer config and one scheduler per graph,
rather than two schedulers arguing over the same packages.

## Caching one opaque unit

The nested repo is cached like any other workspace: Lattice hashes the inputs you
declare, and on a hit restores the outputs and skips the command. A hit means the
inner runner is never invoked at all. A miss hands the whole subtree back to it,
and its own cache decides what to redo inside.

Because a nested repo owns its own layout, `inputs` for it is usually broad
(`["**/*"]`), with `ignore` carrying the precision. Three kinds of paths have to
stay out of the key:

| Ignore | Why |
| --- | --- |
| `**/node_modules/**`, vendored dep trees | Derived and machine-specific. Lockfiles present in the workspace root are hashed automatically, so the installed tree adds nothing. |
| The inner runner's cache (`**/.turbo/**`, `.gradle/`, …) | It changes on every run. Left in the key, no run ever hits. |
| Output directories (`**/dist/**`) | An output inside `inputs` invalidates the very key that produced it. |

`inputs`, `outputs`, and `ignore` are declared per task, not per workspace, so
these globs apply repo-wide. Keep them broad enough to cover the nested repo and
specific enough that the ignores above hold.

## Limitations

### Manual workspaces must declare every task you invoke directly

Run `lattice run lint` and a manual workspace with no `lint` script is an error, not
a skip. A nested repo rarely has every task the rest of the repo does, so either give
it a script or scope the run: `lattice run lint --filter api`. Tasks pulled in only
as dependencies are skipped where they don't apply.

### Don't consume an upstream workspace's artifacts at build time

A workspace's cache key covers the inputs it declares, and `inputs` globs cannot
reach outside its directory. If a downstream workspace copies the nested repo's
bundle into its own outputs, a change confined to the nested repo rebuilds that
workspace and still serves the consumer a cache hit, leaving the copy stale. Read
across workspace boundaries at run time instead, in a task with `"cache": false`.
