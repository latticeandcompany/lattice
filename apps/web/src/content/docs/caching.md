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
- the resolved value of every environment variable named in `env`
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
exported into the task's process when it runs. A variable that is not set
contributes nothing to the key, exactly as if it were not listed.

The failure modes match `inputs`: an omitted variable that changes behavior
gives you a stale hit, and a listed variable the command never reads forces
misses on otherwise-identical work.

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

Inside it, entries are grouped by the cache format they were written in. When a
release changes what goes into a key, it writes under a new format and the old
group is retired by the next `lattice prune` — so an upgrade means one full
rebuild rather than a risk of reading entries whose keys meant something else.

That directory is safe to delete at any time. The next run has nothing to
restore and starts from a clean cache.

## Pruning the cache

The cache only grows; nothing evicts an entry on its own. Run `lattice prune` to
evict the oldest-used entries until the store is back under a size limit:

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
least-recently-used: every hit refreshes an entry's last-used time, so `prune`
removes whichever entries have gone longest unused, stopping as soon as the
total is under budget.
