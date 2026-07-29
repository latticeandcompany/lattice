---
title: Cache internals
description: Exact key composition, on-disk layout, and the lookup and eviction sequences.
group: Reference
order: 6
---

# Cache internals

This page documents the exact mechanics of the on-disk cache: what goes into a
key, how entries are stored and verified, and how eviction works. It is written
for a reader who needs to reason about the cache directly — debugging a phantom
hit, auditing what leaks into a key, or planning a remote backend. Storage is a
trait (`CacheStore`), and `LocalStore` is the only implementation today, with a
future remote backend (Lattice Cloud) in mind. If you just want to know what
Lattice caches and how to control it, see [Caching](/lattice/docs/caching).

## Cache key composition

A task's cache key is a 64-character lowercase hex SHA-256 digest, computed by
`compute_key`. The hasher consumes fields in this fixed order:

1. `lattice_version` — the running Lattice version.
2. `task` — the task name (e.g. `build`).
3. `command` — the fully resolved shell command for this task in this
   workspace.
4. `toolchain_identity` — the identity string of the workspace's resolved
   toolchains (empty if the workspace declares none).
5. `env.name` / `env.value` — one pair per name listed in the task's `env`,
   resolved from the process environment, sorted by name.
6. `input.path` / `input.content` — one pair per file matched by the task's
   `inputs` globs (after removing anything matched by `ignore`), sorted by
   path relative to the workspace, with the file's full contents hashed in.
7. `lockfile.name` / `lockfile.content` — one pair for each tool-unique
   lockfile that exists in the workspace, checked in this fixed order:
   `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `bun.lockb`,
   `bun.lock`, `Cargo.lock`, `go.sum`, `poetry.lock`, `uv.lock`,
   `Gemfile.lock`.

Every list (env pairs, input files, lockfiles present) is sorted before
hashing, so the key does not depend on filesystem iteration order or on the
order fields were declared in `lattice.json`.

### Domain separation and length prefixing

Each field above is written into the hasher as a *tag* plus a *payload*, and
both are length-prefixed:

```text
u64_le(len(tag))  ++  tag_bytes  ++  u64_le(len(payload))  ++  payload_bytes
```

This is why two different input sets can never collide by concatenation. Without
length prefixes, hashing `task="ab"` next to `command="c"` would produce the
same bytes as `task="a"` next to `command="bc"`. With an explicit length in
front of every tag and every payload, a field's boundary is unambiguous
regardless of what bytes it contains — a byte sequence that looks like the next
tag is just payload, because the reader already knows how many bytes to
consume. The tag itself (`"task"`, `"input.path"`, `"lockfile.content"`, …)
further guarantees a value hashed under one field name can never be mistaken
for a value under another.

## On-disk layout

Every cache entry lives under the configured cache directory (default
`.lattice/cache`, overridable with `settings.cacheDir`) as two files sharing
the key as their stem:

```text
.lattice/cache/<key>.tar.gz       the artifact: gzip-compressed tar of outputs
.lattice/cache/<key>.meta.json    the metadata: everything needed to verify and restore
```

There is no subdirectory nesting and no sharding by key prefix — the cache
directory is a flat list of `<key>.tar.gz` / `<key>.meta.json` pairs.

## The metadata file

`CacheMeta` is serialized as pretty-printed, camelCase JSON. This is a real
file written by running a task with `outputs: ["dist/**"]`:

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

Fields:

| Field | Meaning |
| --- | --- |
| `key` | The cache key. Also the filename stem for both files on disk. |
| `task` | The task name, for humans reading the cache directory. |
| `workspace` | The workspace name, for humans reading the cache directory. |
| `durationMs` | How long the task took to run when this entry was written. |
| `lastUsed` | RFC 3339 timestamp, updated on every write and every hit. Drives eviction order — see below. |
| `env` | The resolved `(name, value)` pairs for the task's declared `env`, captured so a cache hit can report what was in effect. |
| `outputDigest` | SHA-256 hex of the `.tar.gz` bytes, recorded when the artifact is written and checked on every lookup. |

Note that `key` and `outputDigest` are both 64-character hex SHA-256 digests,
but they hash different things: `key` is the identity computed by
`compute_key` from the fields above; `outputDigest` is a digest of the stored
artifact's bytes, computed after the tarball is written.

## Archive format and the output digest

`store` collects every file matched by the task's `outputs` globs (a directory
pattern like `dist/**` captures every file beneath it) and writes them,
relative to the workspace root, into a gzip-compressed tar archive at
`<key>.tar.gz`. Files are collected and added in sorted order, so the archive
is reproducible for identical outputs. Once the archive is finished, its bytes
are hashed and the result is written into `meta.outputDigest` before the
metadata file is saved. `restore` reverses this: it opens the tarball and
unpacks it directly into the target workspace path.

## The lookup sequence

`lookup(key)` runs these checks in order. Any failure is a miss — never an
error and never a stale hit:

1. **Read and parse `<key>.meta.json`.** Missing file, or a file that fails to
   parse as JSON → miss.
2. **Check `<key>.tar.gz` exists.** Meta present but no tarball → miss.
3. **Open the tarball and compute its SHA-256.** Unreadable file → miss.
4. **Compare that digest against `meta.outputDigest`.** Mismatch → miss.

Only when all four checks pass does `lookup` return `Some(CacheEntry)`. This is
the guarantee worth internalizing: **a lookup is a hit only if the meta parses,
the tarball opens, and its digest matches the recorded `outputDigest`.** A
truncated download, a manually edited archive, or a half-written file from a
crashed process is a miss, not a false hit — the runner falls through and reruns
the task rather than restoring corrupt output.

## `lastUsed` bookkeeping and eviction order

`lastUsed` is set when an entry is stored and refreshed by `touch`, which the
runner calls on every cache hit after a successful restore. It is the only
field `touch` changes; `outputDigest` and the rest of the metadata are left
alone.

`prune(max_bytes)` enforces `settings.maxCacheSize` (or `--max-size` on
`lattice prune`) by:

1. Scanning the cache directory for every `<key>.meta.json`, reading each
   entry's `lastUsed` and combined on-disk size (`.tar.gz` + `.meta.json`).
2. If the total is already at or under `max_bytes`, doing nothing.
3. Otherwise sorting entries by `lastUsed` ascending and deleting the oldest
   first — removing both files for each evicted key — until the running total
   is at or under `max_bytes`.

This is strict least-recently-used eviction: an entry that is hit often stays,
however old it is, because every hit advances `lastUsed`; an entry nobody has
restored since it was written is the first to go. A missing cache directory is
not an error — `prune` reports zero entries removed and zero bytes freed. If
neither `--max-size` nor `settings.maxCacheSize` is set, `lattice prune` fails
outright rather than picking an arbitrary limit.

## What is deliberately not part of the key

The key is computed from the fields listed above and nothing else. Notably
absent:

- **The task's `outputs` patterns.** Only `inputs` (and lockfiles) are hashed;
  changing what a task declares as output does not change its identity, and
  does not invalidate an existing cache entry for the same inputs.
- **Files outside `inputs`.** A file the workspace contains but that no
  `inputs` glob matches has no effect on the key, even if the task's command
  happens to read it. If a task depends on a file, it belongs in `inputs`.
- **Environment variables not named in the task's `env` list.** Only the
  variables a task explicitly declares are resolved and hashed; anything else
  in the ambient environment is invisible to the cache, by design — otherwise
  every shell's ambient state would perturb every key.
- **Wall-clock time, hostname, or working directory path.** The key is a pure
  function of the fields above, which is what makes it reproducible across
  machines and across clones of the same repository.

The consequence in each case is the same shape: anything not in the key can
change silently without a cache miss. Getting the task's `inputs` (and `env`)
list right is what keeps the cache honest — an incomplete `inputs` list is the
usual cause of a stale hit that should have been a miss.
