---
title: Caching
description: Why Lattice skips work it has already done, what a cache hit actually promises, and what it costs you when the promise is wrong.
group: Concepts
order: 3
---

# Caching

Lattice caches by content, not by timestamp. Before it runs a task, it hashes
everything that can change what that task produces and looks the hash up. A
match restores the recorded outputs and skips the command. No match runs the
command and records the result under the new hash.

Nothing about *when* a file changed matters, which has consequences you notice
within a day of using it. Touch a file without editing it and the task still
hits. Check out an old branch and the build you ran on it last week comes back,
because the content is the same content. Revert a commit and you get the result
from before it rather than a rebuild.

For the exact byte layout of the hash and the on-disk format of an entry, see
[Cache internals](/lattice/docs/cache-internals). For the fields, see
[Configuration](/lattice/docs/configuration).

## What a hit promises, and what it does not

A hit means the specific things Lattice hashed have not changed. It does not
mean the workspace is otherwise the same as last time. The set is:

- the workspace, the task name, and the fully resolved command
- the contents of every file matched by `inputs`, minus anything `ignore` removes
- whether each of those files is executable, and, for a symlink, the path it
  points at rather than the bytes on the other end
- the cache key of every task this one depends on
- the contents of the manifest the command resolves through, so editing the
  `build` script in `package.json` changes the work even though `npm run build`
  is the same string
- the resolved values of the variables named in `env` and `globalEnv`
- the contents of every file matched by `globalDependencies`
- any lockfile in the workspace, or at the repo root
- the identity of the resolved toolchain
- the operating system, the architecture, and the shell
- the running Lattice version

Everything else is invisible to the cache. That is the deal you are making when
you declare `inputs`: you are telling Lattice which files matter, and it
believes you. Get the list wrong in the direction of too narrow and you get a
stale hit, which is the one failure mode worth being afraid of, because the
restored output looks exactly like a correct one. Get it wrong in the direction
of too broad and you get rebuilds you did not need, which is annoying and
harmless.

Leave `inputs` off entirely and Lattice hashes the whole workspace, minus what
your `.gitignore` files exclude and minus the task's own outputs. That is slower
than a narrow list and it re-runs more often than it strictly has to. It is also
the safe default, and it is the default for exactly that reason: an undeclared
task should re-run too eagerly rather than too rarely.

## Why a change upstream re-runs everything downstream

A task's key includes the keys of every task it depends on. Edit a library and
every app that builds against it re-runs, whether or not the app reads the file
you touched.

This is coarse on purpose. A workspace is one node in the graph, so when its key
moves, Lattice knows something in it changed and nothing more. It cannot tell
whether the change reached the particular symbols a dependent imports, because
answering that would mean understanding the language, the module system, and the
build tool of every ecosystem it supports. The honest options are to re-run
dependents whose inputs might have moved, or to serve a build made against code
you have since changed. Lattice re-runs.

If a dependency edge is costing you more than it protects you from, the fix is
in the graph rather than in the cache: a `dependsOn` that is not real is an edge
worth deleting. See [Task graph](/lattice/docs/task-graph).

## Outputs are what a hit gives you back

`outputs` names the files a successful run captures into the stored artifact,
and it is the only thing a hit restores. A file the command produces that no
`outputs` pattern matches is never saved, so a later hit will not put it back,
and whatever consumes it will find it missing.

A restore also clears what `outputs` matches before it unpacks. That is a
correctness decision rather than tidiness. Without it, a hit would layer the
stored tree on top of whatever is already on disk, so a file the task learned to
stop emitting would survive forever, and content-hashed filenames like
`app.a1b2.js` would pile up across every build you ever cached. Clearing first
means a hit reproduces the run rather than approximating it.

A task that declares `outputs` and produces none of them is not cached, and the
run warns. An empty artifact would verify perfectly on every future lookup, so
the task would report a hit, restore nothing, and never run again. That is a
worse outcome than not caching, so Lattice refuses.

Patterns that matched only empty directories get the same refusal. A bare
`outputs: ["dist"]` matches `dist/` itself, so an empty `dist/` looks like a
match even though the task wrote nothing. Storing that would be worse than
storing nothing, because every later hit would restore an empty `dist/` over
whatever a real run had put there:

```text
outputs ["dist"] matched only empty directories, so nothing was cached. Check that the task writes its files where the patterns point
```

## A damaged entry is a miss, never a wrong answer

A lookup is a hit only if the metadata parses, the tarball is the size the
metadata recorded, and the tarball's sha256 matches the digest recorded when it
was written. Anything else falls through and the task runs.

The check is there because the cache is a directory on a disk, and directories
on disks get truncated downloads, interrupted writes, and the occasional
well-meaning `mv`. The worst thing a build cache can do is hand back output that
is not what its key says it is. Verifying the digest on every lookup bounds the
cost of any damage to a single re-run.

## Why the Lattice version is part of every key

Every key includes the version of Lattice that computed it. So a release that
changes what a key covers moves every key with it, and the entries written by
the previous version are never asked for again.

The cost is one full rebuild after an upgrade. The alternative is a release that
reads an entry whose key was computed from a different list of inputs, which is
a stale hit produced by the tool itself rather than by a mistake in your config.
One rebuild is cheap. That class of bug is not. The stranded entries age out
through `lattice prune` like anything else.

## Two ways to not cache

`cache: false` on a task means never look up and never store. Reach for it when
a task's result genuinely is not a function of its inputs: something that reads
the clock, hits the network, or writes to a place outside the workspace.

`persistent: true` disqualifies a task from caching on its own, no `cache: false`
needed, because a dev server produces a running process rather than a result.
See [Persistent tasks](/lattice/docs/persistent-tasks).

The two per-run escapes are different in a way that matters when you suspect an
entry is wrong. `--no-cache` skips the lookup and stores nothing, so the entry
you were suspicious of is still there to be served on the next run.
`--force` skips the lookup and stores the result, replacing it. If you think the
cache is lying to you, `--force` is the one that fixes it. Both are in the
[CLI reference](/lattice/docs/cli).

## The cache directory is disposable

Entries live under `.lattice/cache` by default, flat, two files per key. You can
delete the directory at any point. The next run has nothing to restore and
starts over, which is the only thing losing a cache can ever cost you.

That property is not an accident of the layout. It is the reason the cache lives
inside the repo instead of in a shared location under your home directory: a
cache you can delete by deleting one directory is a cache you can reason about,
and `rm -rf .lattice` is a complete reset rather than the first step of one.

Setting `settings.maxCacheSize` puts a ceiling on the directory, and every run
enforces it by evicting the least recently used entries until the total is under
budget. There is no default ceiling, because Lattice has no way to know whether
this is a laptop with 80 GB free or a CI container with 2. A tool that guessed a
number would be wrong on one of them, and being wrong in the direction of
deleting your cache is worse than growing a directory you can see. Set a budget
when you want one. Run [`lattice prune`](/lattice/docs/cli) to sweep by hand.

## Reading a miss

Lattice hashes each part of the key separately and then hashes the parts
together, which keeps the key one number while letting a miss name the part that
moved. `lattice run build -v` is the only place Lattice reports which part. The
live display shows a hit's key on the task's own line and leaves the trace to
`-v`.

```text
lattice: app:build: cache miss: inputs changed
lattice: web:build: cache miss: globalDependencies changed
lattice: api:test: cache miss: dependencies, env changed
```

The names are the parts listed above. Two misses have no part to name, and they
say so instead of guessing:

```text
lattice: app:build: cache miss (nothing cached for this task yet)
lattice: app:build: cache miss (the entry for this key is no longer in the cache)
```

The second one is what an eviction or a rejected corrupt entry looks like from
the outside. The key did not move; the entry it named is gone.

When a run is nothing but hits, the summary says so:

```text
lattice: full cache, nothing to run
```

One miss, one failure, or one persistent task in the graph withholds it. See
[Output and logging](/lattice/docs/output-modes).

## Where to look next

Field names, types, and defaults are in
[Configuration](/lattice/docs/configuration). The key's byte layout, the
metadata format, the lookup order, and the eviction rules are in
[Cache internals](/lattice/docs/cache-internals). If a task is hitting when you
expected a miss, [Troubleshooting](/lattice/docs/troubleshooting) works through
the usual causes.
