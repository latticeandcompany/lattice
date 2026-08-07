---
title: Caching
description: What Lattice hashes for a task, what a hit restores, and what makes a run miss.
group: Concepts
order: 3
---

# Caching

Before running a task, Lattice computes a key from everything that can change
its result — the command, the files it reads, the environment variables it
depends on, and the resolved toolchain. If that exact key was ever produced
before, Lattice restores the recorded outputs and skips the command. If not, it
runs the command and records the result under the new key.

For the byte layout of the key and the on-disk artifact format, see
[Cache internals](/lattice/docs/cache-internals).

## What feeds the key

A task's cache key is a hash over:

- the workspace it runs in, the task name, and its fully resolved command
- the contents of every file matched by `inputs`, minus anything matched by
  `ignore`
- the cache key of every task it depends on, so a change upstream reaches
  everything downstream of it
- the contents of the manifest the command resolves through — `npm run build`
  names a script in `package.json`, `make test` names a target in a `Makefile`,
  and changing that script changes the work
- the resolved value of every environment variable named in `env`, and in the
  repo-wide `globalEnv`
- the contents of every file matched by the repo-wide `globalDependencies`
- the identity of the resolved toolchain (see
  [Engines and provisioning](/lattice/docs/engines))
- the operating system, architecture, and shell
- the running Lattice version
- the contents of any lockfile in the workspace *or at the repo root*, so a
  dependency bump invalidates the cache even if you never listed the lockfile in
  `inputs`, and even in a layout that keeps one lockfile at the top (the
  internals page has the
  [full lockfile list](/lattice/docs/cache-internals#cache-key-composition))

Nothing outside that set is consulted. A hit tells you those specific things
have not changed, not that the workspace is otherwise identical to last time.

## Dependencies

A task's key includes the keys of everything it depends on. Edit a library and
the apps that build against it re-run, because the library's key moved and the
apps' keys are built from it.

This is deliberately conservative. A workspace is one node, so when its key
moves Lattice cannot tell which of its outputs changed — only that something
did. A dependent re-runs even if the specific files it reads are untouched. The
alternative is serving a build that was made against code you have since
changed.

## Declaring inputs

`inputs` is the set of files whose contents make the key:

```json
{
  "tasks": {
    "build": {
      "inputs": ["src/**/*", "package.json"],
      "outputs": ["dist/**"]
    }
  }
}
```

Miss a file the command actually reads and you get a stale hit — the recorded
output is restored even though that file changed. Glob in files the command
never reads, like fixtures, and every incidental change to them forces a rebuild
that did not need to happen.

Leave `inputs` off and the whole workspace is hashed, minus anything your
`.gitignore` files exclude and minus the task's own `outputs`. That is slower
than a narrow list and it re-runs more often than it strictly needs to, which is
the right way round to be wrong. Declare `inputs` when you want the speed.

You never need to list a task's `outputs` in `ignore`. Outputs are excluded from
the key automatically — otherwise writing them would move the key the run was
about to store under, and the task could never hit its own entry.

## Excluding noise with `ignore`

`ignore` removes files from the set `inputs` matched, without changing the
`inputs` patterns themselves:

```json
{
  "tasks": {
    "test": {
      "inputs": ["src/**/*", "tests/**/*"],
      "ignore": ["**/*.log", "**/*.snap.tmp"]
    }
  }
}
```

Use it for files a broad glob sweeps in that do not affect the result — logs,
scratch files, anything your task writes into its own source tree as a side
effect. A file matched by `ignore` never touches the key, so editing it never
invalidates the cache.

## Declaring outputs

`outputs` is what gets captured into the cached artifact on a successful run,
and what a later hit restores:

```json
{
  "tasks": {
    "build": {
      "inputs": ["src/**/*"],
      "outputs": ["dist/**"]
    }
  }
}
```

A directory pattern like `dist/**` captures every file beneath it, and a bare
`dist` does the same. A file your command produces that no `outputs` pattern
matches is never saved, so a hit will not restore it and a later step depending
on it will find it missing. Only a successful run is stored. `outputs` is
optional; a task without any still caches on success, it just has nothing to
restore.

Symlinks come back as symlinks and directories that end up empty are recreated,
so a restored tree matches what the run produced. A hit also clears what
`outputs` matches before unpacking: a file the task deletes stays deleted, and
content-hashed names like `app.a1b2.js` do not pile up across builds.

If a task declares `outputs` and produces none of them, nothing is cached and
the run warns. That combination almost always means the patterns are wrong —
they are relative to the workspace, not the repo root — and caching an empty
artifact would turn every later run into a hit that restores nothing.

## Declaring env

`env` names environment variables that affect the task's result but are not
files — an API base URL, a target platform, a feature flag:

```json
{
  "tasks": {
    "build": {
      "inputs": ["src/**/*"],
      "env": ["NODE_ENV", "TARGET_PLATFORM"]
    }
  }
}
```

Each name is resolved from your current environment when the key is computed,
and the resolved value — not just the name — is hashed in. The same names are
exported into the task's process when it runs. A name that resolves to nothing is
still hashed as declared-and-unset, so adding one to the list moves the key even
before the variable exists, and setting it later moves the key again.

The failure modes match `inputs`: an omitted variable that changes behavior
gives you a stale hit, and a listed variable the command never reads forces
misses on otherwise-identical work.

## Files shared across workspaces

`inputs` patterns are relative to the workspace the task runs in, which means
they cannot name a file that lives above it. A base `tsconfig.json` at the repo
root, a shared schema directory, a root `.env` — each of these is read by tasks
in several workspaces, and none of them can be written as an `inputs` pattern
that means the same thing in every one.

`globalDependencies` is the repo-root-relative list that covers them. Every file
it matches is hashed into the key of every task:

```json
{
  "globalDependencies": ["tsconfig.base.json", "proto/**", ".env"],
  "tasks": {
    "build": { "inputs": ["src/**/*"], "outputs": ["dist/**"] }
  }
}
```

Patterns work the way `inputs` patterns do: `proto/**` covers the subtree, and a
bare `proto` naming a directory covers it too. Editing anything they match makes
every task miss, so keep the list to files that genuinely cross workspace
boundaries. Without one, a change to a shared root file is invisible to the
cache and every task hits with an artifact built before the edit.

## Repo-wide environment variables

`globalEnv` is `env` at the repo level: names whose resolved values are hashed
into every task's key, for variables that change what any build produces.

```json
{
  "globalEnv": ["NODE_ENV", "CI"]
}
```

A task's own `env` list still applies on top, and is the better place for
anything only that task reads. Unlike `env`, `globalEnv` names are not exported
into task processes — they are already in the environment Lattice inherited.

## Why a task missed

`lattice run build -l` reports each miss with the part of the key that moved:

```text
app:build: cache miss: inputs changed
web:build: cache miss: globalDependencies changed
api:test: cache miss: dependencies, env changed
```

The names match the parts listed under
[what feeds the key](#what-feeds-the-key): `inputs`, `env`, `globalEnv`,
`globalDependencies`, `dependencies` (a prerequisite's key moved), `manifests`,
`toolchain`, `command`, `patterns` (the glob lists themselves changed), and
`environment` (platform, shell, or Lattice version).

Two misses have no part to name. A task that has never completed reports
`nothing cached for this task yet`. A task whose key is unchanged but whose entry
is gone — evicted by a prune, or rejected as corrupt — says so directly.

## Opting out of caching

Set `cache: false` on a task to skip caching entirely — always run, never store,
never look up:

```json
{
  "tasks": {
    "lint:watch": {
      "cache": false
    }
  }
}
```

`persistent: true` tasks are never cached regardless of `cache`, since a dev
server or watcher has no single result to record. See
[Persistent tasks](/lattice/docs/persistent-tasks) for what else that implies
for a run.

## Bypassing the cache for one run

`lattice run build --no-cache` ignores the cache for that invocation entirely:
no lookup, and nothing stored afterward either. Use it to check what a run does
without touching what is already stored.

`lattice run build --force` also skips the lookup, but it does store the result,
replacing whatever was there. That is the one to reach for when you think an
entry is wrong: `--no-cache` re-runs and leaves the suspect entry in place to be
served again next time, while `--force` overwrites it.

## When the whole run is a hit

A run where every scheduled task came back from cache ends with a `FULL CACHE`
line under the summary, or `lattice: full cache — nothing to run` when output is
plain. One miss, one failure, or a `persistent: true` task in the graph is
enough to withhold it. See [Output and logging](/lattice/docs/output-modes) for
how it renders in each mode.

## Corrupt entries are misses

A lookup is a hit only if the stored metadata parses, the stored tarball opens,
and its sha256 digest matches the digest recorded when it was written. Anything
else — no metadata, no tarball, a tarball that will not open, or one whose bytes
do not match — falls through and the task re-runs. A damaged cache entry costs
you a re-run; it cannot hand back the wrong output.

## Where the cache lives

By default, artifacts and their metadata live under `.lattice/cache` at the repo
root. Move it with `settings.cacheDir`:

```json
{
  "settings": {
    "cacheDir": "custom-cache"
  }
}
```

Inside it, entries sit flat, one `.tar.gz` and one `.meta.json` per key. The
running Lattice version is part of every key, so a release that changes what a
key means moves every key with it: an upgrade costs one full rebuild rather than
risking a hit on an entry whose key meant something else, and the entries it left
behind age out through `lattice prune`.

That directory is safe to delete at any time. The next run has nothing to
restore and starts from a clean cache.

## Keeping the cache to a size

Set `settings.maxCacheSize` and every run holds the cache to it:

```json
{
  "settings": {
    "maxCacheSize": "10GB"
  }
}
```

Eviction is least-recently-used: every hit refreshes an entry's last-used time,
so the entries that go are whichever have gone longest unused, stopping as soon
as the total is under budget. The budget covers stored artifacts and their
metadata.

Set no budget and the cache grows without limit — which is fine on a laptop with
room to spare, and is why there is no default. Run `lattice prune` to sweep it
by hand, either against the configured budget or against one given on the spot:

```sh
lattice prune --max-size 10GB
```

With no `--max-size` and no `settings.maxCacheSize`, `prune` fails rather than
guessing at a limit.

`prune` also reclaims what nothing can read: entries from an earlier cache
format, and artifacts left without metadata by an interrupted run. It touches
only the cache's own directories, so a `cacheDir` pointing somewhere shared
keeps whatever else is in there.
