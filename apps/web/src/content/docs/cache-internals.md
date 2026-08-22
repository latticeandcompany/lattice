---
title: Cache internals
description: Cache key composition, on-disk layout, the metadata format, and the lookup and eviction sequences.
group: Reference
order: 6
---

# Cache internals

The exact on-disk cache: what goes into a key, how entries are stored and
verified, and how eviction works. For what caching means for a run, see
[Caching](/lattice/docs/caching).

Storage sits behind one trait, and a local directory is its only implementation
today.

## Cache key composition

A task's cache key is a 64-character lowercase hex SHA-256 digest. It is built
in two stages. Each component below is hashed on its own, then the ten component
digests are hashed together in this fixed order to produce the key.

The two-stage form exists so a miss can name a cause. A key alone reports that
something changed. Comparing components against the ones the task last resolved
to reports which. That comparison produces `cache miss: inputs changed`.

1. `environment`. The running Lattice version, the platform as `<os>-<arch>`,
   the shell (`sh -c` or `cmd /C`), the workspace's declared name, and the task
   name. The workspace name is included so two workspaces running the same
   command with nothing else to distinguish them do not share one entry. The
   Lattice version is included so a release that changes this list never reads
   an entry keyed under the old one.
2. `command`. The fully resolved shell command for this task in this workspace.
3. `toolchain`. The identity string of the workspace's resolved toolchains, empty
   when the workspace declares no engines.
4. `dependencies`. One entry per task this task depends on, each that task's own
   resolved cache key, sorted and deduplicated.
5. `patterns`. The raw `inputs`, `outputs`, and `ignore` glob strings as
   declared, or the literal `<unset>` when a field is absent. Widening `outputs`
   therefore produces a different key rather than hitting an entry that captured
   the narrower set.
6. `env`. One entry per name listed in the task's `env`, sorted by name. The name
   is hashed whether or not the variable is set. A set variable contributes its
   resolved value and an unset one a distinct marker, so declaring a name is
   itself a change to the key.
7. `globalEnv`. The same, for the names listed in the repo-level `globalEnv`.
8. `inputs`. One `path` and `content` pair per input file, sorted by path
   relative to the workspace, with the file's full contents hashed in. The set is
   the files matched by `inputs`, or every file in the workspace that the
   applicable `.gitignore` files do not exclude when `inputs` is absent. In both
   cases anything matched by `ignore` or by the task's own `outputs` is removed
   first. `.lattice`, `.git`, `.hg`, `.svn`, and `.jj` are never walked, and
   symlinks are not followed.
9. `manifests`. Three passes, in this order:
   - One `name` and `content` pair for each manifest present in the workspace. A
     resolved command is usually an indirection: `npm run build` names a script
     in `package.json` and `make test` names a target in a `Makefile`, so the
     command string alone does not pin the work.
   - One pair for each dependency-state file present in the workspace, checked in
     the fixed order listed below.
   - The same list again at the repo root, when the workspace is not itself the
     root.
10. `globalDependencies`. A digest over the repo-level `globalDependencies`
    pattern list plus the path and contents of every repo-root-relative file it
    matches. The pattern list is hashed even when it matches nothing. This digest
    is the same for every task in a run and is computed once, before scheduling.

The dependency-state file order for component 9:

```text
package-lock.json    yarn.lock            pnpm-lock.yaml
bun.lockb            bun.lock             Cargo.lock
go.sum               poetry.lock          uv.lock
Gemfile.lock         npm-shrinkwrap.json  deno.lock
pdm.lock             Pipfile.lock         requirements.txt
Podfile.lock         packages.lock.json   composer.lock
mix.lock             pubspec.lock         Package.resolved
stack.yaml.lock      cabal.project.freeze
```

The same list decides whether `lattice setup` reinstalls dependencies, so the two
cannot disagree.

The repo-root pass exists for hoisted layouts. A pnpm, yarn, or npm workspace, a
Cargo virtual workspace, and a Go workspace all keep the only lockfile at the
top, leaving no lockfile beside the workspace. Without the second pass a
dependency bump in those layouts would invalidate nothing.

Env pairs, input files, and dependency keys are sorted before hashing. Manifests
and lockfiles are visited in their fixed table order. The key therefore does not
depend on filesystem iteration order, on the order fields were declared in
`lattice.json`, or on the order prerequisites finished in.

### Key breakdowns

Alongside the entries, the cache directory keeps one JSON file per
`(workspace, task)` pair recording the component digests that pair last resolved
to:

```text
.lattice/cache/fingerprints/<id>.json
```

`<id>` is a truncated hash of the workspace and task names, so either half may
contain a path separator. The file holds two keys: `key`, the cache key the pair
last resolved to, and `components`, a map from each of the ten component names to
that component's digest.

It is written after a task runs, staged and renamed like the metadata. On a miss,
the current components are compared against it and the differing names are
reported.

These files are a few hundred bytes each, bounded by the number of
workspace-task pairs, and are not counted against `settings.maxCacheSize`.
Deleting them costs the next miss's reason and nothing else.

### Domain separation and length prefixing

Each field is written into the hasher as a tag plus a payload, both
length-prefixed:

```text
u64_le(len(tag))  ++  tag_bytes  ++  u64_le(len(payload))  ++  payload_bytes
```

Without the lengths, `task="ab"` beside `command="c"` would hash the same bytes
as `task="a"` beside `command="bc"`. With an explicit length in front of every
tag and payload, a field boundary is unambiguous whatever bytes the payload
contains. The tag (`"task"`, `"input.path"`, `"lockfile.content"`, and so on)
keeps a value hashed under one field name from reading as a value under another.

## On-disk layout

Every cache entry lives directly under the configured cache directory, default
`.lattice/cache` and overridable with `settings.cacheDir`, as two files sharing
the key as their stem:

```text
.lattice/cache/<key>.tar.gz       the artifact: a gzip-compressed tar of outputs
.lattice/cache/<key>.meta.json    the metadata needed to verify and restore it
```

There is no nesting and no sharding by key prefix. The directory is a flat list
of `<key>.tar.gz` and `<key>.meta.json` pairs beside the `fingerprints/`
directory.

`lattice prune` removes cache entries, orphaned artifacts, and leftover staging
files. It removes no directories. `cacheDir` can point at a directory Lattice
does not own outright, and `.lattice` itself also holds the provisioned
toolchains and the managed binary.

Both files are written to a temporary name in the same directory and renamed
into place. A rename is atomic, so a concurrent reader sees either the previous
file or the complete new one, and two `lattice` processes storing the same key
cannot interleave into one broken entry.

## The metadata file

Metadata is pretty-printed camelCase JSON. This is a real file, written by a task
declaring `outputs: ["dist/**"]` and `env: ["NODE_ENV"]`:

```json
{
  "key": "4063b4e10078320c7a8d8fd97a5e2a7c27cbc1dbdcec82380a700081a0858502",
  "task": "build",
  "workspace": "app",
  "durationMs": 6,
  "lastUsed": "2026-08-21T19:30:40.501574Z",
  "env": {
    "NODE_ENV": "prod"
  },
  "outputDigest": "f9da043330ca748d3e9e01d6cee83e80f5295313f4453cf87a7398f2da305c58",
  "outputs": [
    "dist/**"
  ],
  "artifactSize": 106
}
```

| Field | Meaning |
| --- | --- |
| `key` | The cache key. Also the filename stem for both files. |
| `task` | The task name. |
| `workspace` | The workspace name. |
| `durationMs` | How long the task took when this entry was written. |
| `lastUsed` | RFC 3339 timestamp, set on write and refreshed on every hit. Drives eviction order. |
| `env` | The resolved name and value pairs for the task's declared `env`, as they were when the key was computed. The key is a hash, so this is the only place those values remain legible. |
| `outputDigest` | SHA-256 hex of the `.tar.gz` bytes, recorded when the artifact is written and checked on every lookup. |
| `outputs` | The task's `outputs` globs as they were when the entry was written. A restore clears what these match before unpacking. |
| `artifactSize` | Byte length of the `.tar.gz`. Checked before the digest. |

`key` and `outputDigest` are both 64-character hex SHA-256 digests over different
things. `key` is the identity computed from the components above.
`outputDigest` is a digest of the stored artifact's bytes, computed after the
tarball is written.

## Archive format and the output digest

A store collects everything matched by the task's `outputs` globs and writes it,
relative to the workspace root, into a gzip-compressed tar archive at
`<key>.tar.gz`. A directory pattern like `dist/**` captures every file beneath it.
A pattern with no glob characters that names a directory, like `dist`, expands to
the same thing. Directories and symlinks are recorded as themselves rather than
flattened, so an empty output directory survives a round trip and a symlink comes
back a symlink rather than a copy of its target.

Entries are collected and added in sorted order. Once the archive is finished,
its bytes are hashed into `outputDigest`, then the archive is renamed into place
and the metadata is saved.

A task that declares `outputs` and matches none of them is not stored. An empty
archive would verify on every later lookup, so the task would report a hit,
restore nothing, and never run again. The runner surfaces the refusal as a
warning and the run continues uncached.

A restore opens the tarball, deletes what the entry's recorded `outputs` match,
then unpacks into the workspace. Clearing first is what makes a hit reproduce the
run: a file the task deleted stays deleted, and content-hashed names do not
accumulate across builds. A hit produces files only. No process runs, so the
entry's recorded `env` is not exported into anything.

## The lookup sequence

A lookup runs these checks in order:

1. Read and parse `<key>.meta.json`. A missing file is a miss. So is one that
   fails to parse.
2. Check that `<key>.tar.gz` exists. Metadata with no tarball is a miss.
3. Compare the tarball's length against `artifactSize`. A mismatch is a miss.
   This is the cheap check and it catches the common damage.
4. Compute the tarball's SHA-256. An unreadable file is a miss.
5. Compare that digest against `outputDigest`. A mismatch is a miss.

A lookup returns an entry only when every check passes. A truncated download, a
manually edited archive, or a half-written file from a crashed process makes the
runner fall through and re-run the task. A restore that fails partway is also a
miss. A damaged entry costs one re-run and cannot return the wrong output.

## `lastUsed` bookkeeping and eviction order

`lastUsed` is set when an entry is stored and refreshed on every cache hit after
a successful restore. It is the only field a refresh changes.

Pruning enforces `settings.maxCacheSize`, or `--max-size` on `lattice prune`, in
four steps:

1. Reclaim what can never be read: artifacts with no metadata beside them, and
   leftover temporary files. Both are left by an interrupted store. This happens
   first, because pruning enumerates by metadata and would otherwise never see an
   orphaned artifact.
2. Scan the cache directory for every `<key>.meta.json`, reading each entry's
   `lastUsed` and combined on-disk size. An entry whose metadata no longer parses
   is evicted here rather than aborting the prune.
3. Stop if the total is at or under the limit.
4. Otherwise sort entries by `lastUsed` ascending and delete the oldest first
   until the running total is at or under the limit.

Each eviction removes the metadata before the artifact. Without metadata the
entry is already a miss, so a failure between the two deletions leaves something
the next prune can still find and reclaim. The reverse order would leave
metadata pointing at nothing and, on a failure, leak the artifact permanently.

Eviction is strict least-recently-used. An entry hit often stays however old it
is, because every hit advances `lastUsed`. An entry nobody has restored since it
was written goes first.

A missing cache directory is not an error; the prune reports zero entries removed
and zero bytes freed. With neither `--max-size` nor `settings.maxCacheSize` set,
`lattice prune` fails rather than choosing a limit.

## Not part of the key

The key is computed from the ten components above and nothing else. Five
exclusions are worth stating.

**A task's own output files.** They are excluded even when `inputs` matches them.
Hashing them would move the key the run was about to store under, so the task
could never hit its own entry. The `outputs` patterns themselves are hashed; the
files they match are removed from the input set.

**Files a task reads but does not declare.** When `inputs` is declared it is the
whole input set, so a file the command reads that no glob matches has no effect
on the key. Omit `inputs` and the whole workspace is hashed instead. Either way
the walk stops at the workspace directory. A file above it is covered only by
`globalDependencies`.

**Undeclared environment variables.** Only names listed in a task's `env` or the
repo's `globalEnv` are resolved and hashed. The rest of the ambient environment
would otherwise perturb every key from every shell. The user's global gitignore
is excluded on the same grounds: it lives outside the repo, so honoring it would
make a key depend on whose machine computed it.

**A task's `timeout`.** It bounds how long the task may run. It does not change
what the task produces, so an entry stored under one limit is valid under
another.

**Wall-clock time, hostname, and absolute paths.** Input paths are hashed
relative to the workspace, so the same commit produces the same keys in a
different checkout directory. The platform is hashed, deliberately: a cache
directory shared between machines must not answer one operating system's lookup
with another's artifacts.

Anything absent from the key can change without producing a miss. An incomplete
`inputs`, `env`, or `globalDependencies` list is the usual cause of a stale hit.
