---
title: Upgrading
description: Moving a repo to a new Lattice version and keeping a team on one build.
group: Overview
order: 4
---

# Upgrading

`latticeVersion` in `lattice.json` names the Lattice build this repo runs on.
`lattice upgrade` writes that pin; every other command only reads it.

## `lattice upgrade`

```sh
lattice upgrade 0.2.0
lattice upgrade latest
```

Given a version, `lattice upgrade`:

1. Downloads that release's binary into `.lattice/bin` (skipped if it is
   already there — see [Local binaries][bins] below).
2. Points the `.lattice/bin/lattice` symlink at it.
3. Rewrites `latticeVersion` in `lattice.json`, editing the file as text so key
   order and formatting stay as they were.

`lattice upgrade latest` resolves the newest *stable* release. If the project
has not shipped a stable release yet, it pins the newest pre-release and says so
in its output.

A version may be given with or without a leading `v` (`0.2.0` or `v0.2.0`), and
is otherwise validated as semver: anything else is rejected with `'<value>' is
not a version`. The value ends up in a URL and a filename, so a path like
`../../etc` does not get past this.

When it finishes, `lattice upgrade` prints what changed, plus the exact path to
invoke the new binary (`./.lattice/bin/lattice`, or its absolute path if you are
outside the repo) if it is not the version you are currently running:

```text
◆ lattice  upgrade
  0.1.0 → 0.2.0

lattice.json now pins 0.2.0. Commit it so the whole repo moves together.
Run ./.lattice/bin/lattice to use it.
```

Commit the updated `lattice.json`. That line is what moves the rest of the team.

### Local binaries under `.lattice/bin`

A repo keeps one binary per version it has ever pinned, named
`lattice-<version>`, plus a `lattice` symlink pointing at the current one.
Moving between versions already on disk — switching branches, undoing an
upgrade — is a symlink swap; only a missing version is downloaded. `rm -rf
.lattice` removes every locally installed Lattice build along with the rest of
the directory.

A downloaded archive is checked against the release's published checksum before
it is installed. A mismatch fails the upgrade outright (`checksum mismatch`) and
leaves both the binary and `lattice.json` untouched.

Re-running `lattice upgrade` for a version that is already pinned and already
installed downloads nothing. Lattice still relinks `.lattice/bin/lattice` to it
and reports `already on <version>`, which fixes a stale symlink left behind by a
branch switch.

## The version-drift check

Once a version is pinned, every other command compares it against the binary
that was invoked — a branch switch, a fresh clone, or a colleague's `lattice
upgrade` leaves you holding a different build than the repo pins. What happens
next depends on where that binary came from.

**A binary Lattice installed under `.lattice/bin`** is switched automatically:
the invocation installs the pinned version if needed, relinks
`.lattice/bin/lattice` to it, and hands the command over to it in place, so it
runs as if you had invoked the pinned build directly. The switch is already
underway by the time anything prints:

```text
◆ lattice  0.1.0 · this repo pins 0.2.0 · switching
```

If the pinned version cannot be installed (no network, no matching release),
the command fails rather than silently running the wrong one:

```text
this repo pins lattice 0.2.0, which is not installed and could not be fetched.
Run with --no-version-check to use lattice 0.1.0 anyway
```

**Any other binary** — a `cargo install` build, a debug build on `PATH`, a
package manager's copy — is never replaced or switched; Lattice does not
overwrite a binary it did not put there. Instead, on an interactive terminal
running `lattice run` or `lattice setup`, it prints a one-line advisory nag and
proceeds with the version you invoked:

```text
◆ lattice 0.1.0 · this repo pins 0.2.0 · run `lattice upgrade 0.2.0`
```

The nag never blocks the run, and never appears in CI or raw output — see
[Output and logging][output-modes] for how that mode is chosen.

### Silencing it

The automatic switch and the advisory nag read the same three opt-outs, checked
in this order:

| Opt-out | Scope |
| --- | --- |
| `--no-version-check` | This invocation only |
| `LATTICE_NO_VERSION_CHECK` (any value) | Every invocation in this shell/environment |
| `"settings": { "versionCheck": false }` in `lattice.json` | Every invocation in this repo, for everyone |

With any of these set, a command runs as invoked, drift and all — no switch, no
nag. `versionCheck` defaults to `true`.

## `latticeVersion`: what it does and does not enforce

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "0.2.0",
  "workspaces": []
}
```

`latticeVersion` is read straight out of the JSON, before `lattice.json` is
otherwise parsed or validated against the schema, so a config written for a
newer schema can still say which version is able to read it.

Declaring it drives the switch and nag above, and it is the field `lattice
upgrade` rewrites. It does not pin or validate anything about the *schema*
`lattice.json` is written against — there is no per-version schema
compatibility check beyond what this binary's own parser accepts. Nor does it
block a command from running under a different version once the check is
silenced. It is never required: a `lattice.json` with no `latticeVersion` has
nothing to drift from.

## What an upgrade means for the cache

The running Lattice version is one of the inputs hashed into every task's cache
key, alongside its command, its input files, and its resolved toolchains.
Changing the version changes every key, so the first run after an upgrade misses
across the board: every task re-executes and repopulates the cache under its new
key. Old entries are not lost; they stop being looked up, and are evicted like
any other unused entry. See [Caching][caching] for the rest of what the key is
built from and how eviction works.

[bins]: #local-binaries-under-latticebin
[output-modes]: /lattice/docs/output-modes
[caching]: /lattice/docs/caching
