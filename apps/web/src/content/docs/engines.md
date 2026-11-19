---
title: Engines and provisioning
description: How an engine entry's shape picks host, check, or install, and what Lattice says when a tool is wrong.
group: Concepts
order: 5
---

# Engines and provisioning

An engine is a versioned tool a workspace's tasks run with: `node`, `rust`,
`go`, a linter, anything whose version matters. Declare engines under the root
`engines` key, where they apply to every workspace, and change them for some
workspaces in `overrides`.

The shape of what you write decides what Lattice does. The engine's name decides
only whether the bare-string form is allowed and whether Lattice already knows a
version command for it. An engine never decides which tool runs a workspace's
tasks; that is [driver detection](/lattice/docs/drivers).

## Three modes, chosen by shape

| You write | Mode | What happens |
| --- | --- | --- |
| Nothing, or an empty object | **host** | Uses whatever is on `PATH`. Checks nothing |
| A version constraint, no `installCmd` | **check** | Runs the version command on the host and stops the run if the result does not satisfy the constraint |
| An `installCmd` | **install** | Runs `installCmd` once into `.lattice/toolchains`, checks the result, and puts its `bin` on every task's `PATH` |

### Check the host tool

```json
{ "engines": { "node": ">=22" } }
```

Before any task runs, Lattice runs `node --version`, reads a version out of the
output, and checks it against `>=22`. On a machine with Node 20 the run stops
before anything starts:

```text
error: workspace 'web' cannot run
  engine 'node' does not match the version this repo pins
    wanted   >=22
    found    20.11.1 at /usr/local/bin/node
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

Version parsing is loose, because tools disagree about how to print one: the
first run of digits and dots counts, so `v20.11.1`, `go1.22` and `rustc 1.75.0
(abc 2024)` all parse. A constraint that is a semver range is matched as one; a
bare version such as `"22.11.0"` means `^22.11.0`.

Check mode reads each machine's own `PATH`, so a constraint that passes on one
machine and fails on another is behaving as written. An `installCmd` is what
makes every machine agree.

### Have Lattice install it

```json
{
  "engines": {
    "just": {
      "version": ">=1.30.0",
      "installCmd": "curl -fsSL https://just.systems/install.sh | bash -s -- --to \"$LATTICE_TOOLCHAIN_DIR/bin\""
    }
  }
}
```

`installCmd` opts the engine into installing. Lattice runs it into a directory
it controls, checks the version of what it installed, and reuses the result on
every later run. `bin` names the directory inside the install that holds the
executables, and defaults to `bin`.

## String form and object form

`"node": ">=22"` is the string form. It works for the 40 engines Lattice has a
version rule for, listed on
[Toolchains](/lattice/docs/toolchains#well-known-engines). Any other tool needs
the object form, `{ version, versionCmd, installCmd, bin }`:

```json
{ "engines": { "protoc": { "version": ">=25.0", "versionCmd": "protoc --version" } } }
```

A string for an unknown name is rejected when `lattice.json` loads. An object
with a `version` but no `versionCmd` loads, and fails the first run with
`engine 'protoc' has a version constraint but no way to check the installed
version`.

`gradle` and `maven` are checked through the build's wrapper when the engine has
no `versionCmd` of its own: `./gradlew --version`, run inside the workspace,
because the wrapper decides which Gradle the build uses.

## What a failed check says

Every engine failure says what was wanted, what was found and where, the command
that ran, what it printed, and what to do:

| Headline | Meaning |
| --- | --- |
| `is not installed, or not on PATH` | The shell could not find the program |
| `does not match the version this repo pins` | The host tool's version fails the constraint |
| `could not be checked: its version command failed` | The version command exited non-zero; its output follows |
| `could not be checked: Lattice found no version in what it printed` | The output had no version in it |
| `could not be installed: its installCmd exited with code 7` | The install failed; the command and its output follow |
| `was installed, but its version command failed` | The installed tool is not where `bin` says, or does not run |
| `installed a version that does not match its constraint` | The install worked but installed the wrong version |

A run also notes the versions it uses, at the top of the live display and in the
stream with `-v`:

```text
· using node 22.11.0 · just 1.30.0 (installed by Lattice) · go (unchecked)
```

## Where an installed tool lives

```text
.lattice/toolchains/
  just/
    1.30.0-1a2b3c4d/
      bin/
      pins.json
```

The directory is named for the version the tool reported and the first 8 hex
characters of `sha256(installCmd)`, so editing the install command installs into
a new directory rather than reusing a stale one. `pins.json` records the engine,
version, hash and `bin`. Before installing, Lattice reuses a directory whose pin
matches the current hash, re-checking the version where it can.

The install runs in a staging directory and is renamed into place only after its
version check passes, so a failed install leaves nothing behind to trip the next
run. Two `lattice` processes installing the same engine at once each get their
own staging directory. A tool with no way to read its version installs under
`unknown-<hash>`; a `version` constraint on such a tool is an error rather than
a check nobody runs.

`installCmd` receives the target directory twice: as the
`LATTICE_TOOLCHAIN_DIR` environment variable, and by literal substitution of
`$LATTICE_TOOLCHAIN_DIR` in the command before it runs, so either spelling works
on every platform. It runs through the platform shell, `sh -c` on Unix and
`cmd /C` on Windows.

`bin` has to stay inside the install. An absolute `bin` would put a directory
Lattice never installed at the front of every task's `PATH` while the run still
reported an installed toolchain, so it fails at load, and again if a hand-edited
`pins.json` tries it.

## Activation is per task

Checks and installs happen once per distinct set of engines, on the first run,
`setup` or `env` that needs them. Putting a `bin` directory on `PATH` happens per
task: only that task's process gets it. No shell is sourced and no profile is
written, so `rm -rf .lattice/toolchains` is a complete uninstall.

The directories a package manager installed tools into, such as
`node_modules/.bin`, go on the same `PATH` after the pinned tools, never before:
a pin decides which copy of a tool runs. See [Environment
variables](/lattice/docs/environment-variables#tools-the-project-installed).

To use the pinned tools in your own terminal, `lattice env` prints the same
`PATH` as a command your shell can evaluate:

```sh
eval "$(lattice env)"
```

## Root and per-workspace engines

An override's `engines` merge with the root's, its entries winning per name:

```json
{
  "engines": { "node": ">=20", "rust": ">=1.75" },
  "overrides": {
    "web": { "engines": { "node": ">=22" } }
  }
}
```

`web` checks Node against `>=22`; every other workspace against `>=20`. `rust`
applies to `web` unchanged.

## Related pages

[Pinning tool versions](/lattice/docs/pinning-tool-versions) walks through each
intent. [Toolchains](/lattice/docs/toolchains) has the well-known engine list.
[Configuration](/lattice/docs/configuration#engines) is the field reference.
