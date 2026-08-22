---
title: Upgrading
description: Move a repo to a new Lattice version and keep a team on one build.
group: Overview
order: 4
---

# Upgrading

`latticeVersion` in `lattice.json` names the Lattice build this repo runs on.
`lattice upgrade` writes that pin. Every other command only reads it.

## Move the repo to a version

```sh
lattice upgrade 0.2.0
lattice upgrade latest
```

`lattice upgrade` does three things:

1. Downloads that release's binary into `.lattice/bin`, unless it is already
   there.
2. Points the `.lattice/bin/lattice` symlink at it.
3. Rewrites `latticeVersion` in `lattice.json`, editing the file as text so key
   order and formatting stay as they were.

`lattice upgrade latest` resolves the newest stable release. If the project has
not shipped a stable release yet, it pins the newest pre-release and says so.

A version may carry a leading `v` or not, and is otherwise validated as semver.
Anything else is rejected:

```text
'../../etc' is not a version (expected something like 0.2.0)
```

When it finishes, `lattice upgrade` reports what changed:

```text
❖ lattice  upgrade
  0.1.0 → 0.2.0

lattice.json now pins 0.2.0. Commit it so the whole repo moves together.
Run ./.lattice/bin/lattice to use it.
```

That last line appears only when the new pin is not the version you are
currently running. Commit the updated `lattice.json`. That is what moves the
rest of the team.

To fix a stale symlink left behind by a branch switch, run `lattice upgrade`
for the version already pinned. It downloads nothing, relinks
`.lattice/bin/lattice`, and reports `already on <version>`.

### What lives under `.lattice/bin`

A repo keeps one binary per version it has ever pinned, named
`lattice-<version>`, plus a `lattice` symlink pointing at the current one.
Moving between versions already on disk is a symlink swap, so switching
branches or undoing an upgrade costs nothing. Only a missing version is
downloaded.

A downloaded archive is checked against the release's published checksum before
it is installed. A mismatch fails the upgrade with `checksum mismatch for
<asset>`, prints both digests, and leaves the binary and `lattice.json`
untouched.

`rm -rf .lattice` removes every locally installed Lattice build.

## What happens when the running binary is not the pinned one

A branch switch, a fresh clone, or a colleague's `lattice upgrade` can leave
you holding a build the repo does not pin. What Lattice does next depends on
where your binary came from.

**A binary Lattice installed under `.lattice/bin`** is switched
automatically. The invocation installs the pinned version if needed, relinks
`.lattice/bin/lattice`, and hands the command over in place, so it runs as if
you had invoked the pinned build directly:

```text
❖ lattice 0.1.0 · this repo pins 0.2.0 · switching
```

If the pinned version cannot be installed, the command fails rather than run
the wrong one:

```text
this repo pins lattice 0.2.0, which is not installed and could not be fetched.
Run with --no-version-check to use lattice 0.1.0 anyway
```

**Any other binary** is never replaced. A `cargo install` build, a debug build
on `PATH`, or a package manager's copy is left alone, because Lattice does not
overwrite a binary it did not put there. On an interactive terminal running
`lattice run` or `lattice setup`, it prints one advisory line and proceeds with
the version you invoked:

```text
❖ lattice 0.1.0 · this repo pins 0.2.0 · run `lattice upgrade 0.2.0`
```

That line never blocks the run, and it never appears in CI or raw output. See
[Output and logging][output-modes] for how the mode is chosen.

### Turn the check off

The automatic switch and the advisory line read the same three opt-outs:

| Opt-out | Scope |
| --- | --- |
| `--no-version-check` | This invocation only |
| `LATTICE_NO_VERSION_CHECK` (any value) | Every invocation in this environment |
| `"settings": { "versionCheck": false }` in `lattice.json` | Every invocation in this repo, for everyone |

With any of these set, a command runs as invoked. No switch, no advisory line.
`versionCheck` defaults to `true`.

## What `latticeVersion` does not do

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "0.2.0",
  "workspaces": []
}
```

`latticeVersion` is read straight out of the JSON before `lattice.json` is
parsed or validated, so a config written for a newer schema can still say which
version is able to read it.

It does not pin or validate anything about the schema `lattice.json` is written
against. There is no per-version schema compatibility check beyond what the
running binary's own parser accepts. It also does not stop a command from
running under a different version once the check is off. The field is never
required: a `lattice.json` with no `latticeVersion` has nothing to drift from.

## What an upgrade costs the cache

The running Lattice version is one of the inputs hashed into every task's cache
key. Changing the version changes every key, so the first run after an upgrade
misses across the board and repopulates the cache under the new keys. The old
entries are not lost. They stop being looked up, and they are evicted like any
other unused entry. See [Caching][caching].

[output-modes]: /lattice/docs/output-modes
[caching]: /lattice/docs/caching
