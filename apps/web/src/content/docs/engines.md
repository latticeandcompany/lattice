---
title: Engines and provisioning
description: How an engine constraint's shape picks host, validate, or provision.
group: Concepts
order: 5
---

# Engines and provisioning

An engine is a versioned tool a workspace needs to run its tasks — `node`,
`rust`, `go`, a linter, anything with a version that matters. You declare
engines under the root `engines` key, a workspace's own `engines` key, or
both. Lattice never guesses a version policy for you: the *shape* of the
constraint you write is the only thing that decides what Lattice does with
it, and that shape alone selects one of three modes
(`crates/lattice-workspace/src/toolchain.rs:80-100`).

## Three modes, chosen by shape

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, or an empty object | **host PATH** | Trusts whatever is on `PATH`. Installs nothing, checks nothing. |
| A version constraint, no `installCmd` | **validate-only** | Runs a version command against the host tool and fails if it doesn't satisfy the constraint. Installs nothing. |
| An `installCmd` | **provisioned** | Runs `installCmd` into a content-addressed directory, version-checks the result, pins it, and prepends its `bin` dir to the task's `PATH`. |

Nothing else — not the engine's name, not whether it's a well-known tool —
changes the mode. Only the presence of a version constraint and the presence
of `installCmd` do.

### Host PATH: no constraint, no install command

```json
{
  "engines": {
    "go": {}
  }
}
```

Lattice runs `go` tasks with whatever `go` resolves to on `PATH`. It never
runs a version command and never provisions anything. This is also what
happens if a workspace declares no `engines` at all — there is nothing to
validate, so nothing is checked.

### Validate-only: a version constraint, no install command

```json
{
  "engines": {
    "node": ">=20.0.0"
  }
}
```

Before running any task that needs `node`, Lattice runs `node --version` on
the host, parses the version out of the output, and checks it against
`>=20.0.0`. If the host has Node 18, the run fails before any task starts:

```text
engine 'node' 18.19.0 on PATH does not satisfy constraint '>=20.0.0'
```

(`crates/lattice-workspace/src/toolchain.rs:283-286`.) Nothing is installed —
validate-only mode only ever checks.

### Provisioned: an `installCmd`

```json
{
  "engines": {
    "just": {
      "version": ">=1.30.0",
      "installCmd": "curl -fsSL https://just.systems/install.sh | bash -s -- --to \"$LATTICE_TOOLCHAIN_DIR/bin\"",
      "versionCmd": "just --version",
      "bin": "bin"
    }
  }
}
```

Adding `installCmd` is what opts an engine into provisioning — the version
constraint and `bin` work the same as validate-only, but now Lattice runs
`installCmd` itself, into a directory it controls, instead of checking
whatever is already on `PATH`. The result is pinned so a second run reuses it
instead of reinstalling. See [Where a provisioned tool
lives](#where-a-provisioned-tool-lives) below for the exact layout.

## String form versus object form

`"node": ">=20.0.0"` is the string form: a bare version constraint, nothing
else. It only works for a fixed set of tools Lattice already knows how to
version-check — `WELL_KNOWN_ENGINES` in `crates/lattice-config/src/lib.rs:13`
(`node`, `rust`, `go`, `python`, `java`, and the rest of that list; the full
set is the [Toolchains](/lattice/docs/toolchains) reference). Anything else
must use the object form:

```json
{
  "engines": {
    "protoc": { "version": ">=25.0", "versionCmd": "protoc --version" }
  }
}
```

The object form is `{ version, versionCmd, installCmd, bin }`, all optional
(`crates/lattice-config/src/lib.rs:76-83`). `versionCmd` exists because
Lattice can't guess how to check the version of a tool it doesn't know —
guessing would mean silently trusting an unverifiable claim, so it errors
instead.

Writing the string form for an engine outside `WELL_KNOWN_ENGINES` is
rejected when `lattice.json` is loaded, before any task runs:

```text
engine 'protoc' in root uses the string (version-only) form, but 'protoc'
is not a well-known engine Lattice can version-check on its own. Use the
object form with an explicit `versionCmd`, e.g. "protoc": { "version":
">=1.0.0", "versionCmd": "protoc --version" }
```

(`crates/lattice-config/src/lib.rs:304-316`.) If you use the object form but
still omit `versionCmd` for a tool Lattice has no built-in rule for, the
failure moves to run time instead, when Lattice actually needs to check the
version:

```text
engine 'protoc' has a version constraint but no way to check it (not a
well-known engine and no `versionCmd`)
```

(`crates/lattice-workspace/src/toolchain.rs:276-279`.) Either way, an unknown
tool with a version constraint needs an explicit `versionCmd` — there's no
path that lets Lattice skip it.

## Where a provisioned tool lives

A provisioned engine installs under `.lattice/toolchains/<engine>/`, in a
directory named after the version Lattice resolved and the first 8 hex
characters of `sha256(installCmd)`:

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/          # prepended to the task's PATH
      pins.json
```

While `installCmd` is running, Lattice hasn't resolved a version yet, so it
builds into a temporary `tmp-<hash>` directory first and renames it to
`<version>-<hash>` only after the freshly installed tool passes its version
check (`crates/lattice-workspace/src/toolchain.rs:314-374`). `pins.json`
records what produced that directory:

```json
{
  "engine": "just",
  "version": "1.30.0",
  "installHash": "1a2b3c4d",
  "bin": "bin"
}
```

(`ToolchainPins`, `crates/lattice-workspace/src/toolchain.rs:111-118`.) Before
installing anything, Lattice looks for an existing `<version>-<hash>`
directory whose `pins.json` matches the current `installCmd` hash and whose
`bin` directory still exists — and, when a version command is available,
re-checks that the installed version still satisfies the constraint. A match
is reused; installation happens exactly once per distinct `installCmd`
content (`crates/lattice-workspace/src/toolchain.rs:194-240`).

Hashing on the literal `installCmd` string, rather than on the engine name
and version alone, means changing the install command — a different
registry, a different flag — provisions into a new directory instead of
silently reusing a stale one.

### How `installCmd` sees `$LATTICE_TOOLCHAIN_DIR`

Lattice passes the target directory to `installCmd` two ways at once: as the
`LATTICE_TOOLCHAIN_DIR` environment variable, and by substituting the literal
string `$LATTICE_TOOLCHAIN_DIR` inside the command before running it
(`crates/lattice-workspace/src/toolchain.rs:325-330`). That's why the `just`
example above works whether the installer reads an env var or expects the
path spelled out on its command line — both resolve to the same directory.

## Activation is per task

Provisioning and version resolution happen once per workspace, memoized: if
two workspaces resolve to the same merged engine map, Lattice provisions it
once and reuses the result for both
(`crates/lattice-runner/src/lib.rs:259-279`). But *activating* a resolved
toolchain — putting its `bin` directory on `PATH` — happens per task. Each
task spawns its own child process, and only that child's environment gets
the provisioned `bin` directories prepended to a `PATH` cloned from the
current one (`crates/lattice-runner/src/lib.rs:588`,
`crates/lattice-runner/src/lib.rs:739-748`).

No shell is sourced and no profile is written — the `PATH` change lives only
in the environment of the one process Lattice spawns for that task. Nothing
about a provisioned engine touches your shell, your global tool installs, or
any other task's environment. It also means removing `.lattice` (or just
`.lattice/toolchains`) is a complete uninstall: every provisioned tool lives
under that tree and nowhere else.

## Root and per-workspace engines

A workspace's `engines` map merges with the root `engines`; the workspace's
entries win per key (`resolve_engines`,
`crates/lattice-config/src/lib.rs:319-325`):

```json
{
  "engines": { "node": ">=20.0.0", "rust": ">=1.75" },
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=22" } }
  ]
}
```

`web` validates Node against `>=22`; every other workspace still validates
against the root's `>=20.0.0`; `rust` applies to `web` unchanged, since `web`
never mentions it. This merge happens before any task in that workspace
runs, whether by `lattice run` or `lattice setup`
(`crates/lattice-runner/src/lib.rs:266`,
`crates/lattice/src/commands/setup.rs:72`) — see
[Workspaces](/lattice/docs/workspaces) for how a workspace's own `engines`
key fits alongside `path`, `auto`, `dependsOn`, and `scripts`.

## Where to go next

This page covers the model: what selects a mode, and what provisioning does
on disk. For the full well-known-engine list and the built-in driver table,
see [Toolchains](/lattice/docs/toolchains). For a task-oriented walkthrough
of pinning a version day to day, see [Pinning tool
versions](/lattice/docs/pinning-tool-versions).
