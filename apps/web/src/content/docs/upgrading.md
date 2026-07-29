---
title: Upgrading
description: Moving a repo to a new Lattice version and keeping a team on one build.
group: Overview
order: 4
---

# Upgrading

`latticeVersion` in `lattice.json` names the Lattice build this repo runs on.
`lattice upgrade` is how that pin changes, and it is the only command that
should change it: everything else — the version-drift check described below —
only reads it.

## `lattice upgrade`

```sh
lattice upgrade 0.2.0
lattice upgrade latest
```

Given a version, `lattice upgrade`:

1. Downloads that release's binary into `.lattice/bin` (skipped if it is
   already there — see [Local binaries][bins] below).
2. Points the `.lattice/bin/lattice` symlink at it.
3. Rewrites `latticeVersion` in `lattice.json` to that version, editing the
   file as text so nothing else in it — key order, spacing, comments-free
   formatting — moves.

`lattice upgrade latest` resolves the newest *stable* release first; if the
project has not shipped a stable release yet, it falls back to the newest
pre-release and says so in its output, so nobody who typed `latest` is
surprised to find a pre-release pinned.

A version may be given with or without a leading `v` (`0.2.0` or `v0.2.0`); it
is otherwise validated as semver and rejected with `'<value>' is not a version`
if it is not — the value ends up in a URL and a filename, so this doubles as
a guard against a path like `../../etc`.

Once it finishes, `lattice upgrade` prints what changed and, if the version
just pinned is not the one you are currently running, a reminder of the exact
path to invoke it (`./.lattice/bin/lattice`, or its absolute path if you are
outside the repo). The command ends there:

```text
◆ lattice  upgrade
  0.1.0 → 0.2.0

lattice.json now pins 0.2.0. Commit it so the whole repo moves together.
Run ./.lattice/bin/lattice to use it.
```

Commit the updated `lattice.json`. That single line is what moves the rest of
the team — the next section is how.

### Local binaries under `.lattice/bin`

A repo keeps one binary per version it has ever pinned, named
`lattice-<version>`, plus a `lattice` symlink pointing at the current one.
Moving between versions already on disk — switching branches, undoing an
upgrade — is a symlink swap; only a version that is missing gets downloaded.
This is part of why `rm -rf .lattice` is a complete reset: it also removes
every locally installed Lattice build.

A downloaded archive is checked against the release's published checksum
before it is installed. A mismatch fails the upgrade outright
(`checksum mismatch`) and leaves both the binary and `lattice.json` untouched.

If you re-run `lattice upgrade` for the version already pinned, and that
version is already installed, nothing is downloaded — Lattice still relinks
`.lattice/bin/lattice` to it and reports `already on <version>`, which covers
the one case where doing nothing would leave the repo pointed at the wrong
binary (for instance, after a branch switch left the symlink stale).

## The version-drift check

Once a version is pinned, every other command checks it against the binary
that was invoked — a branch switch, a fresh clone, or a colleague's `lattice
upgrade` can easily leave you holding a different build than the repo now
pins. What happens next depends on where that binary came from.

**A binary Lattice installed under `.lattice/bin`** is switched automatically:
the invocation installs the pinned version if needed, relinks
`.lattice/bin/lattice` to it, and hands the command over to it in place, so it
runs as if you had invoked the pinned build directly. This is not advisory —
by the time anything prints, the switch is already happening:

```text
◆ lattice  0.1.0 · this repo pins 0.2.0 · switching
```

If the pinned version cannot be installed (no network, no matching release),
the command fails rather than silently running the wrong one, and says so:

```text
this repo pins lattice 0.2.0, which is not installed and could not be fetched.
Run with --no-version-check to use lattice 0.1.0 anyway
```

**Any other binary** — a `cargo install` build, a debug build on `PATH`, a
package manager's copy — is never replaced or switched. Lattice does not
overwrite a binary it did not put there. Instead, on an interactive terminal
running `lattice run` or `lattice setup`, it prints a one-line, advisory nag
and proceeds with the version you invoked:

```text
◆ lattice 0.1.0 · this repo pins 0.2.0 · run `lattice upgrade 0.2.0`
```

This nag never appears in CI or raw output — see [Output and
logging][output-modes] for how that mode is chosen — and it never blocks the
run. It is a suggestion, not a gate.

### Silencing it

Both behaviors above — the automatic switch and the advisory nag — read the
same three opt-outs, checked in this order:

| Opt-out | Scope |
| --- | --- |
| `--no-version-check` | This invocation only |
| `LATTICE_NO_VERSION_CHECK` (any value) | Every invocation in this shell/environment |
| `"settings": { "versionCheck": false }` in `lattice.json` | Every invocation in this repo, for everyone |

With any of these set, a command runs as invoked, drift and all — no switch,
no nag. `versionCheck` defaults to `true`.

## `latticeVersion`: what it does and does not enforce

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "0.2.0",
  "workspaces": []
}
```

`latticeVersion` is read directly out of the JSON, before `lattice.json` is
otherwise parsed or validated against the schema. That is deliberate: a config
written for a newer schema still has to be readable enough to say which
version can read it, even if this binary would otherwise choke on it.

What declaring it does:

- Drives the switch/nag behavior above.
- Is what `lattice upgrade` rewrites.

What it does not do:

- It does not pin or validate anything about the *schema* `lattice.json` is
  written against — there is no per-version schema compatibility check beyond
  whatever this binary's own parser accepts.
- It does not block a command from running under a different version when
  the check is silenced, and it is never required — a `lattice.json` with no
  `latticeVersion` at all is valid; nothing drifts because there is nothing to
  drift from.

## What an upgrade means for the cache

The running Lattice version is one of the inputs hashed into every task's
cache key, alongside its command, its input files, and its resolved
toolchains. Changing that version changes every task's key, so the first run
after an upgrade is a cache miss across the board — every task re-executes
and re-populates the cache under the new key. Nothing is corrupted or lost;
old entries simply stop being looked up, and are evicted the same as any
other unused entry. See [Caching][caching] for the rest of what the key is
built from and how eviction works.

[bins]: #local-binaries-under-latticebin
[output-modes]: /lattice/docs/output-modes
[caching]: /lattice/docs/caching
