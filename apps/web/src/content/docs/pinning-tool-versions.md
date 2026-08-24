---
title: Pinning tool versions
description: What to write under engines for each intent, from a version check to a full install.
group: Guides
order: 4
---

# Pinning tool versions

Every block below is a real `engines` map you can drop into `lattice.json`,
either at the root, where it applies to every workspace, or inside a
`workspaces` entry, where it overrides the root key by key.

The shape of the constraint decides what Lattice does with it. For the model
behind that, see [Engines and provisioning](/lattice/docs/engines). For the
table of built-in engines and drivers, see
[Toolchains](/lattice/docs/toolchains).

## Fail the run when the host tool is too old

Give the engine a bare version constraint and nothing else. Lattice checks the
tool already on `PATH` and installs nothing:

```json
{
  "engines": {
    "node": ">=20.0.0"
  }
}
```

Verify it with any run:

```sh
lattice run build
```

A host tool that satisfies the constraint logs nothing. One that does not fails
the run before any task starts:

```text
Error: engine 'node' on PATH is 18.19.0, which does not satisfy the constraint '>=20.0.0'
```

The constraint is a lower bound on whatever the version command reports, so the
fix on a failing machine is to upgrade the host tool. Lattice installs nothing
in this mode.

The bare-string form works for the 40 names Lattice already knows how to
version-check. That is every built-in driver plus the language toolchains
`rust`, `python3`, `php`, `elixir`, and `haskell`/`ghc`. The [full list with
each version command](/lattice/docs/toolchains#well-known-engines) is on the
Toolchains page. A bare string for any other name is rejected at load time. See
[A tool Lattice does not know](#a-tool-lattice-does-not-know).

## Have Lattice install the tool

Add `installCmd`. Lattice then provisions the tool itself instead of trusting
`PATH`. The install command runs once, into a directory Lattice controls, and
the result is version-checked afterward:

```json
{
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
`PATH`. It defaults to `bin`. `$LATTICE_TOOLCHAIN_DIR` is available both as a
literal substring, substituted before the shell runs the command, and as an
environment variable, so installers of either kind work.

```sh
lattice setup
lattice run build
```

`lattice setup` provisions every declared engine before it installs any
workspace's dependencies, so a package manager that shells out to a provisioned
compiler sees it on `PATH` during install. `lattice run` provisions on demand if
you skip `setup`. A task never runs against a missing or wrong-versioned
toolchain.

The first run logs the provisioning step, then the task:

```text
lattice: provisioning engine 'demo' via installCmd into .lattice/toolchains/demo/tmp-2ee6e363
lattice: running `build` across 1 workspace
```

Every run after that logs nothing, because the existing verified install is
reused. A failing install command stops the run with the installer's own output
underneath:

```text
Error: engine 'node': installCmd failed:
<the installer's stderr>
```

An install that succeeds but still does not satisfy the constraint is also
fatal:

```text
Error: engine 'node' provisioned 19.8.1, which does not satisfy the constraint '>=20.0.0'
```

Both of those failures are in the `installCmd` itself: network access, a dead
URL, or a platform-specific installer.

### Where the provisioned tool lands

Every provisioned engine goes under `.lattice/toolchains/<engine>/`, inside the
repo:

```text
.lattice/toolchains/demo/1.2.3-2ee6e363/
  bin/          prepended to the task's PATH
  pins.json     the version installed and the hash that produced it
```

```json
{
  "engine": "demo",
  "version": "1.2.3",
  "installHash": "2ee6e363",
  "bin": "bin"
}
```

The hash in the directory name is the first 8 hex characters of
`sha256(installCmd)`. Change the `installCmd` and Lattice provisions into a new
directory rather than mutating the old one.

The version in the directory name is the one the tool reported once Lattice had
installed it. An engine with no `versionCmd` and no built-in version rule has
nothing to report, so it lands under `unknown-<hash>` and the hash identifies it.
A `version` you have no way to check is an error instead, because Lattice does
not record a version it did not read.

## Give each workspace its own version

Declare `engines` inside a workspace instead of, or in addition to, the root. A
workspace's entries override the root key by key, and the rest of the root map
still applies:

```json
{
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

`legacy-api` accepts whatever `node` on `PATH` satisfies `>=18.0.0`, and `web`
requires `>=20.0.0`. Both check the same `PATH`, so this works only if one host
`node` satisfies both.

To give each workspace its own copy, add an `installCmd` per workspace:

```json
{
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

The provisioned directory is addressed by the content of the resolved
`installCmd`, so two workspaces asking for the same engine name with different
install commands provision into two directories under
`.lattice/toolchains/node/`, and each task gets only its own workspace's `bin`
directory on `PATH`. Verify one at a time:

```sh
lattice run build --filter legacy-api
lattice run build --filter web
```

## A tool Lattice does not know

An engine name outside the well-known list has no built-in version rule, so the
bare string form is rejected and the config fails to load:

```json
{
  "engines": { "alpes": ">=2.0.0" }
}
```

```text
Error: engine 'alpes' in root uses the string form, which carries only a version. 'alpes' is not a well-known engine, so Lattice cannot version-check it on its own. Use the object form with a `versionCmd`, like this: "alpes": { "version": ">=1.0.0", "versionCmd": "alpes --version" }
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

That is validate-only, the same as a well-known engine with a bare string. Add
`installCmd` to provision it instead, exactly as in [Have Lattice install the
tool](#have-lattice-install-the-tool).

Lattice reads the first version-looking substring `versionCmd` prints, so
`v2.6.7`, `alpes 2.6.7 (build 41)`, and `2.6` all parse. The command only needs
to print a version somewhere in its output.

## Remove a provisioned toolchain

`.lattice/toolchains` and `.lattice/cache` are both regenerable, and `lattice
init` writes both into `.gitignore`. To reset every provisioned tool in the
repo:

```sh
rm -rf .lattice
lattice run build
```

The next run reprovisions each declared `installCmd` and rebuilds the cache.
Nothing outside the repo is touched. That is also the shortest fix for a
provisioned tool in a state you would rather not debug.

Note that `rm -rf .lattice` also removes `.lattice/bin`, where Lattice keeps the
versions of itself a repo has pinned, and the committed
`.lattice/schema.json`. The next command rewrites the schema, and it
re-downloads the pinned binary. See [Upgrading](/lattice/docs/upgrading).
