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
`compute_key`. The hasher consumes fields in this fixed order:

1. `lattice_version` — the running Lattice version.
2. `task` — the task name (e.g. `build`).
3. `command` — the fully resolved shell command for this task in this workspace.
4. `toolchain_identity` — the identity string of the workspace's resolved
   toolchains (empty if the workspace declares none).
5. `env.name` / `env.value` — one pair per name listed in the task's `env`,
   resolved from the process environment, sorted by name.
6. `input.path` / `input.content` — one pair per file matched by the task's
   `inputs` globs (after removing anything matched by `ignore`), sorted by path
   relative to the workspace, with the file's full contents hashed in.
7. `lockfile.name` / `lockfile.content` — one pair for each dependency-state
   file that exists in the workspace, checked in this fixed order:
   `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`, `bun.lock`,
   `Cargo.lock`, `go.sum`, `poetry.lock`, `uv.lock`, `Gemfile.lock`,
   `npm-shrinkwrap.json`, `deno.lock`, `pdm.lock`, `Pipfile.lock`,
   `requirements.txt`, `Podfile.lock`, `packages.lock.json`, `composer.lock`,
   `mix.lock`, `pubspec.lock`, `Package.resolved`, `stack.yaml.lock`,
   `cabal.project.freeze`. The same list decides whether `lattice setup`
   reinstalls dependencies, so the two never disagree.

Env pairs and input files are sorted before hashing, and lockfiles are visited
in the fixed order listed above, so the key never depends on filesystem
iteration order or on the order fields were declared in `lattice.json`.

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
`.lattice/cache`, overridable with `settings.cacheDir`) as two files sharing the
key as their stem:

```text
.lattice/cache/<key>.tar.gz       the artifact: gzip-compressed tar of outputs
.lattice/cache/<key>.meta.json    the metadata: everything needed to verify and restore
```

There is no subdirectory nesting and no sharding by key prefix. The cache
directory is a flat list of `<key>.tar.gz` / `<key>.meta.json` pairs.

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
  "outputDigest": "7b9b704caefd130afd5440ce20ec161676fe24497cf344eb8473f06cf2984b75"
}
```

| Field | Meaning |
| --- | --- |
| `key` | The cache key. Also the filename stem for both files on disk. |
| `task` | The task name, for humans reading the cache directory. |
| `workspace` | The workspace name, for humans reading the cache directory. |
| `durationMs` | How long the task took to run when this entry was written. |
| `lastUsed` | RFC 3339 timestamp, updated on every write and every hit. Drives eviction order — see below. |
| `env` | The resolved `(name, value)` pairs for the task's declared `env`, captured so a cache hit can report what was in effect. |
| `outputDigest` | SHA-256 hex of the `.tar.gz` bytes, recorded when the artifact is written and checked on every lookup. |

`key` and `outputDigest` are both 64-character hex SHA-256 digests over
different things: `key` is the identity computed by `compute_key` from the
fields above, and `outputDigest` is a digest of the stored artifact's bytes,
computed after the tarball is written.

## Archive format and the output digest

`store` collects every file matched by the task's `outputs` globs (a directory
pattern like `dist/**` captures every file beneath it) and writes them, relative
to the workspace root, into a gzip-compressed tar archive at `<key>.tar.gz`.
Files are collected and added in sorted order, so the archive is reproducible
for identical outputs. Once the archive is finished its bytes are hashed and the
result is written into `meta.outputDigest` before the metadata file is saved.
`restore` opens the tarball and unpacks it directly into the target workspace
path.

## The lookup sequence

`lookup(key)` runs these checks in order:

1. Read and parse `<key>.meta.json`. A missing file is a miss. A file that is
   present but fails to parse as JSON is an error from the store rather than a
   miss: the runner warns (`cache lookup failed: …`) and runs the task.
2. Check `<key>.tar.gz` exists. Meta present but no tarball → miss.
3. Open the tarball and compute its SHA-256. Unreadable file → miss.
4. Compare that digest against `meta.outputDigest`. Mismatch → miss.

`lookup` returns `Some(CacheEntry)` only when all four checks pass: a hit
requires that the meta parses, the tarball opens, and its digest matches the
recorded `outputDigest`. A truncated download, a manually edited archive, or a
half-written file from a crashed process makes the runner fall through and rerun
the task rather than restore corrupt output.

## `lastUsed` bookkeeping and eviction order

`lastUsed` is set when an entry is stored and refreshed by `touch`, which the
runner calls on every cache hit after a successful restore. It is the only field
`touch` changes.

`prune(max_bytes)` enforces `settings.maxCacheSize` (or `--max-size` on
`lattice prune`) by:

1. Scanning the cache directory for every `<key>.meta.json`, reading each
   entry's `lastUsed` and combined on-disk size (`.tar.gz` + `.meta.json`).
2. If the total is already at or under `max_bytes`, doing nothing.
3. Otherwise sorting entries by `lastUsed` ascending and deleting the oldest
   first — removing both files for each evicted key — until the running total is
   at or under `max_bytes`.

Eviction is strict least-recently-used: an entry that is hit often stays however
old it is, because every hit advances `lastUsed`, and an entry nobody has
restored since it was written goes first. A missing cache directory is not an
error; `prune` reports zero entries removed and zero bytes freed. If neither
`--max-size` nor `settings.maxCacheSize` is set, `lattice prune` fails rather
than picking an arbitrary limit.

## Not part of the key

The key is computed from the fields above and nothing else. Four notable
exclusions:

A task's `outputs` patterns are not hashed. Changing what a task declares as
output does not change its identity and does not invalidate an existing entry
for the same inputs.

Files outside `inputs` are not hashed. A file the workspace contains but no
`inputs` glob matches has no effect on the key, even if the command reads it. If
a task depends on a file, it belongs in `inputs`.

Environment variables not named in the task's `env` list are not hashed. Only
declared variables are resolved and hashed; the rest of the ambient environment
would otherwise perturb every key from every shell.

Wall-clock time, hostname, and working directory path are not hashed. The key is
a pure function of the fields above, which is what makes it reproducible across
machines and across clones of the same repository.

Anything not in the key can change without producing a miss, so an incomplete
`inputs` or `env` list is the usual cause of a stale hit.
