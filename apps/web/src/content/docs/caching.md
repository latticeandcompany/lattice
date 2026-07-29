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
before, Lattice restores the recorded outputs and skips the command. If not,
it runs the command and records the result under the new key.

This page covers the decisions you make: what to declare as `inputs`,
`outputs`, `ignore`, and `env`; when to opt a task out of caching entirely;
and where the cache lives on disk. For the byte layout of the key and the
on-disk artifact format, see
[Cache internals](/lattice/docs/cache-internals).

## What feeds the key

A task's cache key is a hash over:

- the task name and its fully resolved command
- every file matched by `inputs`, minus anything matched by `ignore` —
  contents, not just paths
- any tool-unique lockfile present in the workspace (`package-lock.json`,
  `pnpm-lock.yaml`, `Cargo.lock`, `poetry.lock`, and similar), so a dependency
  bump invalidates the cache even if you forgot to list the lockfile in
  `inputs`
- the resolved value of every environment variable named in `env`
- the identity of the resolved toolchain for the task (see
  [Engines and provisioning](/lattice/docs/engines))
- the running Lattice version

Change any one of these and the key changes. Nothing outside this list is
consulted — a cache hit tells you these specific things haven't changed, not
that the workspace is otherwise identical to last time.

## Declaring inputs

`inputs` is the set of files whose contents make the key. Without it, a task
has no files in its key at all: the same command with the same environment
always hits, even after you edit the source it builds from.

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

Get this wrong in either direction and caching misbehaves in a specific way:

- **Under-declare** (miss a file the command actually reads) and you get a
  stale hit: the recorded output is restored even though that file changed.
- **Over-declare** (glob in files the command never reads, like fixtures or
  generated output) and every incidental change to those files forces a
  rebuild that didn't need to happen, and hashing takes longer on every run.

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

Use it for files a broad glob sweeps in that don't affect the result — logs,
scratch files, anything your task writes into its own source tree as a side
effect. A file matched by `ignore` never touches the key, so editing it never
invalidates the cache, even if `inputs` would otherwise match it.

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

A directory pattern like `dist/**` captures every file beneath it. If a file
your command produces isn't matched by `outputs`, it's simply never saved —
a cache hit won't restore it, so a later step that depends on it will find it
missing. Only a successful run is stored; a task that fails is never cached,
matching or not. `outputs` is optional; a task with none still caches on
success, it just has nothing to restore.

## Declaring env

`env` names environment variables that affect the task's result but aren't
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

Each name in `env` is resolved from your current environment when the key is
computed, and that resolved value — not just the name — is hashed in. The
same names are exported into the task's process when it runs. If a named
variable isn't set at all, it contributes nothing to the key, the same as if
it weren't listed; only a variable that's actually set changes the hash.

Get this wrong the same way you'd get `inputs` wrong: leave a variable that
changes the command's behavior off the list, and a run with a different value
for it still comes back as a hit; list a variable the command never actually
uses, and unrelated changes to it force otherwise-identical work to miss.

## Opting out of caching

Set `cache: false` on a task to skip caching entirely — always run, never
store, never look up:

```json
{
  "tasks": {
    "lint:watch": {
      "cache": false
    }
  }
}
```

`persistent: true` tasks are never cached regardless of `cache` — a dev
server or watcher has no single result to record. See
[Persistent tasks](/lattice/docs/persistent-tasks) for what else that
implies for a run.

## Bypassing the cache for one run

`lattice run build --no-cache` ignores the cache for that invocation: no
lookup, and nothing is stored afterward either. `--force` does the same
thing — it's a plain alias, not a stricter mode. Reach for either when you
suspect a stale result and want to confirm by re-running from scratch without
disturbing what's already stored.

## The integrity rule

A lookup is a hit only if the stored metadata parses, the stored tarball
opens, and its sha256 digest matches the digest recorded when it was written.

Any other outcome — no metadata, no tarball, a tarball that won't open, or
one whose bytes don't match the recorded digest — is a miss, never a bad hit.
A corrupted or partially-written cache entry can only cost you a re-run; it
can never hand back the wrong output silently.

## Where the cache lives

By default, artifacts and their metadata live under `.lattice/cache` at the
repo root. Move it with `settings.cacheDir`:

```json
{
  "settings": {
    "cacheDir": "custom-cache"
  }
}
```

`.lattice/cache` (or whatever `cacheDir` points at) is safe to delete at any
time — the next run just has nothing to restore and starts from a clean
cache.

## Pruning the cache

The cache only grows: nothing evicts an entry on its own. Run `lattice
prune` to evict the oldest-used entries until the store is back under a size
limit:

```sh
lattice prune --max-size 10GB
```

Without `--max-size`, `prune` uses `settings.maxCacheSize`:

```json
{
  "settings": {
    "maxCacheSize": "10GB"
  }
}
```

If neither is set, `prune` fails rather than guessing at a limit. Eviction is
least-recently-used: every cache hit refreshes an entry's last-used time, so
`prune` removes whichever entries have gone longest unused first, stopping as
soon as the total is under budget.
