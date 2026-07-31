---
title: Pinning tool versions
description: Get everyone on your team building with the same tool versions.
group: Guides
order: 4
---

# Pinning tool versions

A task-oriented guide to the `engines` field: what to write for each intent. For
the model behind it — the three-mode gradient, `PATH` activation, cache identity —
see [Engines and provisioning](/lattice/docs/engines). For the table of built-in
engines and drivers, see [Toolchains](/lattice/docs/toolchains).

Every block below is a real `engines` map you can drop into `lattice.json`, either
at the root (applies to every workspace) or inside a `workspaces` entry (applies
to that workspace, overriding the root value key by key).

## Fail fast when the host tool is too old

Give the engine a bare version constraint and nothing else. Lattice checks the
tool already on `PATH` and installs nothing:

```json
{
  "latticeVersion": "0.1.0",
  "engines": {
    "node": ">=20.0.0"
  }
}
```

The bare-string form works for the engines Lattice already knows how to
version-check: every built-in driver, plus the language toolchains `rust`,
`python3`, `php`, `elixir`, and `haskell`/`ghc`. The
[full list with each version command](/lattice/docs/toolchains#well-known-engines)
is on the Toolchains page. A bare string for any other name is rejected at load
time, not at run time — see [A tool Lattice doesn't
know](#a-tool-lattice-doesnt-know) below.

Verify it:

```sh
lattice run build
```

A host tool that satisfies the constraint logs nothing. One that doesn't fails the
run before any task starts:

```text
Error: engine 'node' 18.19.0 on PATH does not satisfy constraint '>=20.0.0'
```

The constraint is a lower bound on whatever the version command reports, so the
fix on a failing machine is to upgrade the host tool — `nvm install 20`,
`brew upgrade node`, whatever that platform uses. Lattice installs nothing in this
mode.

## Have Lattice install the tool

Add `installCmd` and Lattice provisions the tool itself instead of trusting
`PATH`. The install command runs once, into a directory Lattice controls, and is
version-checked afterward:

```json
{
  "latticeVersion": "0.1.0",
  "engines": {
    "node": {
      "version": ">=20.0.0",
      "installCmd": "curl -fsSL https://get.volta.sh | VOLTA_HOME=$LATTICE_TOOLCHAIN_DIR bash && $LATTICE_TOOLCHAIN_DIR/bin/volta install node@20",
      "bin": "bin"
    }
  }
}
```

`bin` is the directory, relative to the install, that gets prepended to a task's
`PATH`; it defaults to `bin`. `$LATTICE_TOOLCHAIN_DIR` is available both as a
literal substring, substituted before the shell runs the command, and as an
environment variable, so installers of either kind work.

Run it:

```sh
lattice setup
lattice run build
```

`lattice setup` provisions every declared engine before installing any workspace's
native dependencies, so a package manager that shells out to a provisioned
compiler sees it on `PATH` during install. `lattice run` provisions on demand if
you skip `setup`; a task never runs against a missing or wrong-versioned
toolchain.

The first run logs the provisioning step, then the task:

```text
lattice: provisioning engine 'node' via installCmd into .lattice/toolchains/node/tmp-a1b2c3d4
lattice: running `build` across 1 workspace(s)
```

Every run after that logs nothing — the existing, verified install is reused. A
failing install command stops the run with its output inline:

```text
Error: engine 'node': installCmd failed:
<installer's stderr>
```

An install that succeeds but still doesn't satisfy the constraint is also fatal:

```text
Error: engine 'node' provisioned 19.8.1 does not satisfy '>=20.0.0'
```

Both failures are in the `installCmd` — network access, a dead URL, a
platform-specific installer — not in anything local to the machine that hit them.

### Where the provisioned tool lands

Every provisioned engine goes under `.lattice/toolchains/<engine>/`, inside the
repo:

```text
.lattice/toolchains/node/20.11.1-a1b2c3d4/
  bin/          prepended to the task's PATH
  pins.json     { "engine", "version", "installHash", "bin" }
```

The hash in `20.11.1-a1b2c3d4` is the first 8 hex characters of
`sha256(installCmd)`. Change the `installCmd` and Lattice provisions into a new
directory instead of mutating the old one.

`rm -rf .lattice` removes every provisioned toolchain along with the cache, and
the next `lattice run` or `lattice setup` reprovisions from `installCmd`. That is
also the shortest fix for a provisioned tool in a state you don't want to debug.

## Different versions per workspace

Declare `engines` inside a workspace instead of, or in addition to, the root. A
workspace's entries override the root's key by key; the rest of the root map still
applies:

```json
{
  "latticeVersion": "0.1.0",
  "engines": {
    "node": ">=18.0.0"
  },
  "workspaces": [
    { "name": "legacy-api", "path": "services/legacy-api" },
    {
      "name": "web",
      "path": "apps/web",
      "engines": { "node": ">=20.0.0" }
    }
  ]
}
```

`legacy-api` runs against whatever `node` on `PATH` satisfies `>=18.0.0`, and
`web` requires `>=20.0.0`. Both check the same `PATH`, so this works only if one
host `node` satisfies both. To give each its own copy, add an `installCmd` per
workspace:

```json
{
  "latticeVersion": "0.1.0",
  "workspaces": [
    {
      "name": "legacy-api",
      "path": "services/legacy-api",
      "engines": {
        "node": {
          "version": ">=18.0.0",
          "installCmd": "install-node.sh --version 18 --dest $LATTICE_TOOLCHAIN_DIR"
        }
      }
    },
    {
      "name": "web",
      "path": "apps/web",
      "engines": {
        "node": {
          "version": ">=20.0.0",
          "installCmd": "install-node.sh --version 20 --dest $LATTICE_TOOLCHAIN_DIR"
        }
      }
    }
  ]
}
```

The provisioned directory is content-addressed by the resolved `installCmd`, so
two workspaces asking for the same engine name with different install commands
provision into two directories under `.lattice/toolchains/node/`, and each task
gets only its own workspace's `bin` dir on `PATH`. Verify:

```sh
lattice run build --filter legacy-api
lattice run build --filter web
```

## A tool Lattice doesn't know

An engine name outside the well-known list has no built-in version rule, so the
bare string form is rejected. This config fails to load:

```json
{
  "engines": { "alpes": ">=2.0.0" }
}
```

```text
Error: engine 'alpes' in root uses the string (version-only) form, but
'alpes' is not a well-known engine Lattice can version-check on its own. Use
the object form with an explicit `versionCmd`, e.g. "alpes": { "version":
">=1.0.0", "versionCmd": "alpes --version" }
```

Use the object form and give Lattice the version command:

```json
{
  "engines": {
    "alpes": {
      "version": ">=2.0.0",
      "versionCmd": "alpes --version"
    }
  }
}
```

That is validate-only, the same as a well-known engine with a bare string; add
`installCmd` to provision it instead, exactly as in [Have Lattice install the
tool](#have-lattice-install-the-tool). Lattice parses the first version-looking
substring `versionCmd` prints (`v2.6.7`, `alpes 2.6.7 (build 41)`, `2.6`), so the
command only needs to print a version somewhere in its output.

## Removing a provisioned toolchain

`.lattice/toolchains` and `.lattice/cache` are both regenerable and both excluded
from version control — `lattice init` writes them into `.gitignore`. Deleting
`.lattice` resets every provisioned tool this repo has installed:

```sh
rm -rf .lattice
lattice run build
```

The next run reprovisions each declared `installCmd` and rebuilds the cache.
Nothing outside the repo is touched.
