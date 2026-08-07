---
title: Cache internals
description: Exact key composition, on-disk layout, and the lookup and eviction sequences.
group: Reference
order: 6
---

# Cache internals

This page documents the on-disk cache exactly: what goes into a key, how entries
are stored and verified, and how eviction works. Storage sits behind the
`CacheStore` trait, and `LocalStore` is the only implementation today. For what
Lattice caches and how to control it, see [Caching](/lattice/docs/caching).

## Cache key composition

A task's cache key is a 64-character lowercase hex SHA-256 digest, computed by
`compute_key_detailed`. It is built in two stages: each *component* below is
hashed on its own, then the component digests are hashed together in this fixed
order to produce the key.

The two stages exist so a miss can be attributed. A key on its own can say that
something changed and nothing more; comparing components against the ones the
task last resolved to names which. That comparison is what
`cache miss: inputs changed` reports.

1. `environment` — the on-disk cache format, the running Lattice version, the
   platform as `<os>-<arch>`, the shell (`sh -c` or `cmd /C`), the workspace's
   declared name, and the task name. The workspace name matters on its own:
   without it, two workspaces running the same command with nothing else to tell
   them apart would share one entry and restore each other's artifacts. The cache
   format also names the subdirectory entries are written under, so a release
   that changes this list cannot read entries whose keys were built from the old
   one.
2. `command` — the fully resolved shell command for this task in this workspace.
3. `toolchain` — the identity string of the workspace's resolved toolchains
   (empty if the workspace declares none).
4. `dependencies` — one entry per task this task depends on, each that task's own
   resolved cache key, sorted. This is what carries a change upstream to
   everything downstream of it.
5. `patterns` — the raw `inputs`, `outputs` and `ignore` glob strings as
   declared, or the literal `<unset>` when a field is absent. Widening `outputs`
   therefore produces a different key, rather than hitting an entry that captured
   the narrower set and restoring less than the run made.
6. `env` — one `name`/`value` pair per name listed in the task's `env`, resolved
   from the process environment, sorted by name.
7. `globalEnv` — the same, for the names listed in the repo-level `globalEnv`.
8. `inputs` — one `path`/`content` pair per input file, sorted by path relative
   to the workspace, with the file's full contents hashed in. The set is the
   files matched by `inputs`, or — when `inputs` is absent — every file in the
   workspace that the applicable `.gitignore` files do not exclude. In both cases
   anything matched by `ignore` or by the task's own `outputs` is removed first,
   and `.lattice`, `.git`, `.hg`, `.svn` and `.jj` are never walked. Symlinks are
   not followed.
9. `manifests` — the manifests and lockfiles that pin what the command does:
   - one `name`/`content` pair for each manifest present in the workspace. A
     resolved command is usually an indirection: `npm run build` names a script
     in `package.json` and `make test` names a target in a `Makefile`, so the
     command string alone does not pin the work.
   - one pair for each dependency-state file that exists in the workspace,
     checked in this fixed order: `package-lock.json`, `yarn.lock`,
     `pnpm-lock.yaml`, `bun.lockb`, `bun.lock`, `Cargo.lock`, `go.sum`,
     `poetry.lock`, `uv.lock`, `Gemfile.lock`, `npm-shrinkwrap.json`,
     `deno.lock`, `pdm.lock`, `Pipfile.lock`, `requirements.txt`, `Podfile.lock`,
     `packages.lock.json`, `composer.lock`, `mix.lock`, `pubspec.lock`,
     `Package.resolved`, `stack.yaml.lock`, `cabal.project.freeze`. The same list
     decides whether `lattice setup` reinstalls dependencies, so the two never
     disagree.
   - the same list again at the repo root, when the workspace is not itself the
     root. A layout that hoists one lockfile to the top — pnpm, yarn and npm
     workspaces, a Cargo virtual workspace, a Go workspace — keeps no lockfile
     beside the workspace, so without this a dependency bump would invalidate
     nothing.
10. `globalDependencies` — a digest over the repo-level `globalDependencies`
    pattern list plus the path and contents of every repo-root-relative file it
    matches. The pattern list is hashed even when it matches nothing, so adding a
    pattern is itself a change. This digest is the same for every task in a run
    and is computed once, before scheduling.

Env pairs, input files and dependency keys are sorted before hashing, and
manifests and lockfiles are visited in the fixed order listed above, so the key
never depends on filesystem iteration order, on the order fields were declared in
`lattice.json`, or on the order prerequisites happened to finish in.

### Key breakdowns

Alongside the entries, each format directory keeps one small JSON file per
`(workspace, task)` pair recording the component digests that pair last resolved
to:

```text
.lattice/cache/v3/fingerprints/<id>.json
```

`<id>` is a truncated hash of the workspace and task names, so either half can
contain a path separator. The file is written after a task runs, staged and
renamed like the metadata. On a miss, the current components are compared
against it and the differing names are reported.

These are a few hundred bytes each, bounded by the number of workspace-task
pairs, and are not counted against `settings.maxCacheSize`: evicting one would
cost the explanation and free nothing worth freeing. Deleting them loses only
the next miss's reason.

### Domain separation and length prefixing

Each field is written into the hasher as a *tag* plus a *payload*, both
length-prefixed:

```text
u64_le(len(tag))  ++  tag_bytes  ++  u64_le(len(payload))  ++  payload_bytes
```

Without the lengths, `task="ab"` next to `command="c"` would hash the same bytes
as `task="a"` next to `command="bc"`. With an explicit length in front of every
tag and payload, a field boundary is unambiguous whatever bytes the payload
contains. The tag (`"task"`, `"input.path"`, `"lockfile.content"`, …) keeps a
value hashed under one field name from being read as a value under another.

## On-disk layout

Every cache entry lives under the configured cache directory (default
`.lattice/cache`, overridable with `settings.cacheDir`), inside a subdirectory
named for the cache format, as two files sharing the key as their stem:

```text
.lattice/cache/v3/<key>.tar.gz       the artifact: gzip-compressed tar of outputs
.lattice/cache/v3/<key>.meta.json    the metadata: everything needed to verify and restore
```

Within a format directory there is no further nesting and no sharding by key
prefix: it is a flat list of `<key>.tar.gz` / `<key>.meta.json` pairs, beside the
`fingerprints/` directory described above.

Sibling directories whose names have the shape of a cache format — `v1`, `v2`,
and so on — are entries from other formats; `lattice prune` deletes them
outright, since a key computed under one format never means the same thing under
another. Anything else beside them is left alone: `cacheDir` can point at a
directory Lattice does not own outright, and a prune that swept every neighbour
would take the provisioned toolchains and the installed binary with it.

Both files are written to a temporary name in the same directory and renamed into
place. A rename is atomic, so a concurrent reader sees either the previous file
or the complete new one — never a half-written artifact — and two `lattice`
processes storing the same key cannot interleave into one broken entry.

## The metadata file

`CacheMeta` is serialized as pretty-printed, camelCase JSON. This is a real file
written by running a task with `outputs: ["dist/**"]`:

```json
{
  "key": "b6961d5e2c3f67b5c29bda672621ad835cb286a8da6bd82a6fef01e9cfb372c5",
  "task": "build",
  "workspace": "app",
  "durationMs": 5,
  "lastUsed": "2026-07-29T01:26:44.096642Z",
  "env": {},
  "outputDigest": "7b9b704caefd130afd5440ce20ec161676fe24497cf344eb8473f06cf2984b75",
  "outputs": ["dist/**"],
  "artifactSize": 148
}
```

| Field | Meaning |
| --- | --- |
| `key` | The cache key. Also the filename stem for both files on disk. |
| `task` | The task name, for humans reading the cache directory. |
| `workspace` | The workspace name, for humans reading the cache directory. |
| `durationMs` | How long the task took to run when this entry was written. |
| `lastUsed` | RFC 3339 timestamp, updated on every write and every hit. Drives eviction order — see below. |
| `env` | The resolved `(name, value)` pairs for the task's declared `env`, as they were when the key was computed. The key is a hash, so this is the only place those values are legible afterwards. |
| `outputDigest` | SHA-256 hex of the `.tar.gz` bytes, recorded when the artifact is written and checked on every lookup. |
| `outputs` | The task's `outputs` globs as they were when the entry was written. A restore clears what these match before unpacking, so a hit reproduces the tree the run produced instead of layering onto whatever is already there. |
| `artifactSize` | Byte length of the `.tar.gz`. Checked before the digest, so a truncated artifact is rejected without reading it. |

`key` and `outputDigest` are both 64-character hex SHA-256 digests over
different things: `key` is the identity computed by `compute_key` from the
fields above, and `outputDigest` is a digest of the stored artifact's bytes,
computed after the tarball is written.

## Archive format and the output digest

`store` collects everything matched by the task's `outputs` globs and writes it,
relative to the workspace root, into a gzip-compressed tar archive at
`<key>.tar.gz`. A directory pattern like `dist/**` captures every file beneath it;
a pattern with no glob characters that names a directory, like `dist`, is expanded
to the same thing. Directories and symlinks are recorded as themselves, not
flattened, so an empty output directory survives a round trip and a symlink comes
back a symlink rather than a copy of its target.

Entries are collected and added in sorted order. Once the archive is finished its
bytes are hashed and the result is written into `meta.outputDigest`, then the
archive is renamed into place and the metadata saved.

If a task declares `outputs` but nothing matches them, `store` fails rather than
writing an empty archive. An empty archive would verify perfectly on every later
lookup, so the task would report a hit, restore nothing, and never run again. The
runner surfaces the failure as a warning and the run continues uncached.

`restore` opens the tarball, deletes what the entry's recorded `outputs` match,
then unpacks into the workspace. Clearing first is what makes a hit reproduce the
run: a file the task deleted stays deleted, and content-hashed names do not
accumulate across builds. Files are all a hit gives you — no process runs, so the
entry's recorded `env` is not exported into anything.

## The lookup sequence

`lookup(key)` runs these checks in order:

1. Read and parse `<key>.meta.json`. A missing file is a miss, and so is one that
   fails to parse — metadata nobody can read describes an entry nobody can use,
   so the task simply re-runs and overwrites it.
2. Check `<key>.tar.gz` exists. Meta present but no tarball → miss.
3. Compare the file's length against `meta.artifactSize`. Mismatch → miss. This
   is the cheap check, and it catches the common damage.
4. Compute the tarball's SHA-256. Unreadable file → miss.
5. Compare that digest against `meta.outputDigest`. Mismatch → miss.

`lookup` returns `Some(CacheEntry)` only when every check passes. A truncated
download, a manually edited archive, or a half-written file from a crashed process
makes the runner fall through and rerun the task rather than restore corrupt
output. A restore that fails partway is also a miss: the task runs, so a damaged
entry costs a re-run and can never hand back the wrong output.

## `lastUsed` bookkeeping and eviction order

`lastUsed` is set when an entry is stored and refreshed by `touch`, which the
runner calls on every cache hit after a successful restore. It is the only field
`touch` changes.

`prune(max_bytes)` enforces `settings.maxCacheSize` (or `--max-size` on
`lattice prune`) by:

1. Reclaiming what can never be read: artifacts with no metadata beside them,
   leftover temporary files, and directories belonging to other cache formats.
   This happens first, before the current format's directory is even opened, so a
   repo that has just upgraded — and has no directory for the new format yet —
   still gets the old one retired.
2. Scanning the format directory for every `<key>.meta.json`, reading each
   entry's `lastUsed` and combined on-disk size (`.tar.gz` + `.meta.json`). An
   entry whose metadata no longer parses is evicted here rather than aborting the
   prune.
3. If the total is already at or under `max_bytes`, stopping.
4. Otherwise sorting entries by `lastUsed` ascending and deleting the oldest
   first until the running total is at or under `max_bytes`.

Each eviction removes the metadata before the artifact. That order matters: without
metadata the entry is already a miss, so a failure in between leaves something the
next prune can still find and reclaim. The other order would leave metadata
pointing at nothing and, on a failure, leak the artifact permanently.

Eviction is strict least-recently-used: an entry that is hit often stays however
old it is, because every hit advances `lastUsed`, and an entry nobody has
restored since it was written goes first. A missing cache directory is not an
error; `prune` reports zero entries removed and zero bytes freed. If neither
`--max-size` nor `settings.maxCacheSize` is set, `lattice prune` fails rather
than picking an arbitrary limit.

## Not part of the key

The key is computed from the components above and nothing else. Five notable
exclusions:

A task's own output *files* are not hashed, even when `inputs` matches them.
Hashing them would move the key the run was about to store under, so the task
could never hit its own entry. The `outputs` patterns themselves are hashed; the
files they match are removed from the input set.

Files a task reads but does not declare are not hashed. When `inputs` is declared
it is the whole input set, so a file the command reads but no glob matches has no
effect on the key. Omit `inputs` entirely and the whole workspace is hashed
instead, which is slower but cannot miss a file this way. Either way the walk
stops at the workspace directory: a file above it is covered only by
`globalDependencies`.

Environment variables named in neither the task's `env` nor the repo's
`globalEnv` are not hashed. Only declared variables are resolved and hashed; the
rest of the ambient environment would otherwise perturb every key from every
shell. The user's global gitignore is excluded on the same grounds — it lives
outside the repo, so honoring it would make a key depend on whose machine
computed it.

A task's `timeout` is not hashed. It bounds how long the task may run; it does
not change what the task produces, so an entry stored under one limit is still
valid under another.

Wall-clock time, hostname, and absolute paths are not hashed. Input paths are
hashed relative to the workspace, so the same commit produces the same keys in a
different checkout directory. Note that the *platform* is hashed, deliberately: a
cache directory shared between machines must not answer one operating system's
lookup with another's artifacts.

Anything not in the key can change without producing a miss, so an incomplete
`inputs`, `env` or `globalDependencies` list is the usual cause of a stale hit.
