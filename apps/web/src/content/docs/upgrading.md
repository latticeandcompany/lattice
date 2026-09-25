---
title: Upgrading
description: Move a repo to a new Lattice version and keep a team on one build.
group: Overview
order: 4
---

# Upgrading

`latticeVersion` in `lattice.json` names the Lattice build this repo runs on.
`lattice upgrade` writes that pin. Every other command only reads it.

## Upgrading to 2.0

2.0 changes the shape of `lattice.json`. A 1.x config listed every workspace as
an object with a `name` and a `path`, wrote every dependency edge by hand, and
chose a workspace's tool through `auto`, `scripts` and placeholder `engines`
entries. 2.0 reads workspaces, their dependencies and their tasks from the
repo's own manifests, and `lattice.json` holds only what those cannot say. A 1.x
config does not load:

```text
error: lattice.json is written for Lattice 1.x, which listed every workspace as an object with a name and a path. Run `lattice migrate` to rewrite it for this version
```

From the repo root, with a 2.0 binary:

```sh
lattice migrate --dry-run   # print the new lattice.json, write nothing
lattice migrate             # rewrite it
```

`migrate` runs discovery against your repo and writes down only where the result
would differ from what the 1.x config said, so every workspace keeps its name,
every ordering holds, and every command you wrote still runs:

| 1.x had | 2.0 gets |
| --- | --- |
| A workspace no workspace file names | Its path in `workspaces` |
| A workspace file member the config left out | Its path in `exclude` |
| A name that differs from the manifest's | `overrides.<path>.name` |
| A `dependsOn` a manifest already states | Nothing; the edge is read from the manifest |
| A `dependsOn` no manifest states | `overrides.<name>.dependsOn` |
| `scripts` | `overrides.<name>.tasks` |
| `auto: false` | `"driver": "none"` |
| An engine like `"pnpm": ">=0.0.0"` that only chose a tool | `overrides.<name>.driver`, and the engine dropped |
| Workspace `engines` | `overrides.<name>.engines` |
| `tasks`, `engines`, `globalDependencies`, `globalEnv`, `settings` | Kept as they were |

Before writing, `migrate` parses the new file and discovers the repo with it. It
then says what it dropped:

```text
✓ rewrote lattice.json 64 lines → 21
· dropped the `pnpm` engine: its constraint matched any version, so it only chose a driver, and drivers are chosen with `driver` now
· web: dropped dependsOn ui — its manifest already says so
```

`migrate` always runs as the binary you invoked, even in a repo that pins a 1.x
`latticeVersion`, and it sets `latticeVersion` to its own version. Commit the new
`lattice.json` and `.lattice/schema.json` together.

Things to check afterwards:

- **Run `lattice run build --dry-run`.** Every workspace and command should be
  the one you had.
- **Tasks you never declared now run.** A task no longer has to be listed under
  `tasks`: any script a package declares, and any verb its tool has (`lint` is
  `cargo clippy` in a crate), runs by name. `lattice run lint` in a Cargo crate
  used to run `cargo lint`, which does not exist.
- **`build` caches the tool's build directory by default.** An npm, uv, poetry
  or pdm workspace's `build` stores `dist/**`, Gradle's `**/build/**`, Maven's
  `**/target/**`, .NET's `**/bin/Debug/**` and `**/bin/Release/**`, unless
  `outputs` says otherwise. A cache hit
  clears those directories before restoring them.
- **`dev`, `start`, `serve` and `watch` are persistent** unless a task setting
  says `"persistent": false`.
- **`-v` no longer switches to the line stream.** It adds the trace to either
  display. Scripts that relied on `-v` for parseable output should pass
  `--stream`, or run where stdout is not a terminal, as CI does.
- **Workspaces added later take their manifest's name**, such as `@acme/ui` for
  a scoped package, which is what `--filter` matches.
- **A Gradle, Maven or .NET build is one workspace.** A 1.x config that listed
  Gradle subprojects separately keeps them as listed; list the build root
  instead once you are ready, so one `./gradlew` orders them.

The first run on 2.0 misses the cache everywhere, as every version change does.

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

`lattice upgrade latest` resolves the newest stable release. If there is no
stable release to resolve, it pins the newest pre-release and says so.

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

On Windows every name carries `.exe`, and `lattice.exe` is a copy of the
versioned binary rather than a symlink, because Windows withholds the privilege
a symlink needs. The swap is a file replacement, so it fails while that copy is
running. Close any running `lattice` and upgrade again.

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

That line never blocks the run, and it never appears in CI or the plain stream. See
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
  "latticeVersion": "0.2.0"
}
```

`latticeVersion` is read straight out of the JSON before `lattice.json` is
parsed or validated, so a config written for a newer schema can still say which
version is able to read it. That is how a repo on 2.0 hands a 1.x binary Lattice
installed over to 2.0 before the older binary ever tries to read the new format.

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
