---
title: Engines and provisioning
description: How an engine constraint's shape picks host, validate, or provision.
group: Concepts
order: 5
---

# Engines and provisioning

An engine is a versioned tool a workspace needs to run its tasks: `node`,
`rust`, `go`, a linter, anything whose version matters. Declare engines under
the root `engines` key, under a workspace's own `engines` key, or both.

The shape of the constraint you write selects what Lattice does with it. The
engine's name does not. A name decides only two things: whether the bare-string
form is allowed, and whether Lattice already knows a version command for it.

## Three modes, chosen by shape

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, or an empty object | **host PATH** | Trusts whatever is on `PATH`. Installs nothing, checks nothing. |
| A version constraint, no `installCmd` | **validate-only** | Runs a version command against the host tool and fails if the result does not satisfy the constraint. Installs nothing. |
| An `installCmd` | **provisioned** | Runs `installCmd` into a content-addressed directory, version-checks the result, pins it, and prepends its `bin` directory to the task's `PATH`. |

### Host PATH: no constraint, no install command

```json
{
  "engines": {
    "go": {}
  }
}
```

`go` tasks run against whatever `go` resolves to on `PATH`. No version command,
no provisioning. A workspace that declares no `engines` at all behaves the same
way.

### Validate-only: a version constraint, no install command

```json
{
  "engines": {
    "node": ">=20.0.0"
  }
}
```

Before any task that needs `node` runs, Lattice runs `node --version` on the
host, parses a version out of the output, and checks it against `>=20.0.0`. On a
host with Node 18 the run fails before any task starts:

```text
engine 'node' 18.19.0 on PATH does not satisfy constraint '>=20.0.0'
```

Version parsing is deliberately loose, because tools disagree about how to print
a version. Lattice takes the first run of digits and dots it finds, so
`v20.11.1`, `go1.22`, and `rustc 1.75.0 (abc 2024)` all parse. A constraint that
`semver` can read is matched as a range. A bare or loose constraint such as
`"1.22"` is read as a lower bound. An empty constraint matches anything.

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

`installCmd` is what opts an engine into provisioning. `version` means the same
thing it does in validate-only mode. `bin` is read only here, and it names the
directory inside the install that holds the executables. What changes is that
Lattice runs the install itself, into a directory it controls, instead of
checking `PATH`. The result is pinned, so a second run reuses it. See [Where a
provisioned tool lives](#where-a-provisioned-tool-lives) for the layout.

## String form versus object form

`"node": ">=20.0.0"` is the string form: a bare version constraint. It works
only for the 40 engines Lattice has a built-in version rule for, which include
`node`, `rust`, `go`, `python`, and `java`. The full list is on
[Toolchains](/lattice/docs/toolchains#well-known-engines). Anything else needs
the object form:

```json
{
  "engines": {
    "protoc": { "version": ">=25.0", "versionCmd": "protoc --version" }
  }
}
```

The object form is `{ version, versionCmd, installCmd, bin }`, all optional,
with `bin` defaulting to `bin`. `versionCmd` tells Lattice how to read the
version of a tool it has no rule for.

The string form for an engine outside the well-known list is rejected when
`lattice.json` loads, before any task runs:

```text
engine 'protoc' in root uses the string (version-only) form, but 'protoc'
is not a well-known engine Lattice can version-check on its own. Use the
object form with an explicit `versionCmd`, e.g. "protoc": { "version":
">=1.0.0", "versionCmd": "protoc --version" }
```

The object form with a version constraint but no `versionCmd` parses. It fails
later, when Lattice needs the version:

```text
engine 'protoc' has a version constraint but no way to check it (not a
well-known engine and no `versionCmd`)
```

Either way, an unknown tool with a version constraint needs an explicit
`versionCmd`.

## Where a provisioned tool lives

A provisioned engine installs under `.lattice/toolchains/<engine>/`, in a
directory named for the resolved version and the first 8 hex characters of
`sha256(installCmd)`:

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/
      pins.json
```

The version is not known while `installCmd` is running, so Lattice installs into
a temporary `tmp-<hash>` directory and renames it to `<version>-<hash>` only
after the new tool passes its version check. With no `versionCmd` and no
built-in rule, the version records as `0.0.0`. `pins.json` records what produced
the directory:

```json
{
  "engine": "just",
  "version": "1.30.0",
  "installHash": "1a2b3c4d",
  "bin": "bin"
}
```

Before installing, Lattice looks for an existing `<version>-<hash>` directory
whose `pins.json` matches the current `installCmd` hash and whose `bin`
directory still exists. Where both a version command and a constraint are
available, it re-checks that the installed version still satisfies the
constraint. A match is reused, so installation happens once per distinct
`installCmd`.

The hash covers the literal `installCmd` string rather than the engine name and
version, so editing the install command provisions into a new directory instead
of reusing a stale one.

### How `installCmd` sees `$LATTICE_TOOLCHAIN_DIR`

Lattice passes the target directory to `installCmd` two ways at once: as the
`LATTICE_TOOLCHAIN_DIR` environment variable, and by substituting the literal
string `$LATTICE_TOOLCHAIN_DIR` into the command before running it. The `just`
example above works whether the installer reads an environment variable or
expects the path on its command line.

`installCmd` and `versionCmd` both run through the platform shell, `sh -c` on
Unix and `cmd /C` on Windows, the same way a task's command does.

## Activation is per task

Provisioning and version resolution happen once per distinct merged engine map.
Two workspaces that resolve to the same map provision once and share the result.

Activation, meaning putting a `bin` directory on `PATH`, happens per task. Each
task spawns its own child process, and only that child's environment gets the
provisioned `bin` directories prepended to a `PATH` cloned from the current one.
No shell is sourced and no profile is written, so the change lives and dies with
that one process.

Every provisioned tool lives under `.lattice/toolchains`, which makes
`rm -rf .lattice` a complete uninstall. The next run provisions again from
`installCmd`.

## Root and per-workspace engines

A workspace's `engines` map merges with the root's, and the workspace's entries
win per key:

```json
{
  "engines": { "node": ">=20.0.0", "rust": ">=1.75" },
  "workspaces": [
    { "name": "web", "path": "apps/web", "engines": { "node": ">=22" } }
  ]
}
```

`web` validates Node against `>=22`, and every other workspace validates against
the root's `>=20.0.0`. `rust` applies to `web` unchanged, since `web` never
mentions it. The merge runs before any task in that workspace, under both
`lattice run` and `lattice setup`. See [Workspaces](/lattice/docs/workspaces)
for how a workspace's `engines` key sits alongside `path`, `auto`, `dependsOn`,
and `scripts`.

## Related pages

[Toolchains](/lattice/docs/toolchains) has the full well-known-engine list and
the built-in driver table. [Pinning tool
versions](/lattice/docs/pinning-tool-versions) walks through pinning a version
day to day. [Configuration](/lattice/docs/configuration#engines) is the field
reference for the `engines` map.
