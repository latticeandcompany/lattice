---
title: Pinning tool versions
description: Get everyone on your team building with the same tool versions.
group: Guides
order: 4
---

# Pinning tool versions

This is a task-oriented guide to the `engines` field: what to write for each
intent. For the underlying model — the three-mode gradient, `PATH`
activation, and cache identity — see
[Engines and provisioning](/lattice/docs/engines). For the exhaustive table of
built-in engines and drivers, see [Toolchains](/lattice/docs/toolchains).

Every recipe below is a real `engines` block you can drop into `lattice.json`,
either at the root (applies to every workspace) or inside a `workspaces` entry
(applies to that workspace only, overriding the root value key by key).

## I just want to fail fast if the host tool is too old

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

This only works for the engines Lattice already knows how to version-check:
every built-in driver — `node`, `deno`, `bun`, `pnpm`, `yarn`, `npm`, `cargo`,
`go`, `pip`, `uv`, `poetry`, `pdm`, `pipenv`, `python`, `bundler`, `rake`,
`java`, `kotlin`, `gradle`, `maven`, `dotnet`, `nuget`, `pod`, `swift`,
`composer`, `mix`, `dart`, `stack`, `cabal`, `just`, `task`, `turbo`, `nx` —
plus the language toolchains `rust`, `python3`, `php`, `elixir`, and
`haskell`/`ghc`. The
[full table with each version command](/lattice/docs/toolchains#well-known-engines)
is on the Toolchains page. A bare string for any other name is rejected at load
time, not at run time. If your tool is not on this list, see
["My tool isn't one Lattice knows"](#my-tool-isnt-one-lattice-knows) below.

Verify it:

```sh
lattice run build
```

If the host tool satisfies the constraint, the run proceeds silently — there
is nothing to log, because nothing was installed. If it does not, the run
fails before any task starts:

```text
Error: engine 'node' 18.19.0 on PATH does not satisfy constraint '>=20.0.0'
```

Tell a teammate whose machine fails this check: the constraint is a lower
bound on whatever `node` (or your engine's version command) reports, so the
fix is to upgrade the host tool — `nvm install 20`, `brew upgrade node`,
whatever your platform uses. Lattice does not install anything for you in
this mode; that's the next recipe.

## I want Lattice to install it

Add `installCmd` and Lattice provisions the tool itself instead of trusting
whatever is on `PATH`. The install command runs once, into a directory it
controls, and is version-checked afterward:

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

`bin` is the directory, relative to the install, that gets prepended to a
task's `PATH`; it defaults to `bin` if omitted. `$LATTICE_TOOLCHAIN_DIR` is
available both as a literal substring (substituted before the shell runs the
command) and as an environment variable, so an installer that reads env vars
and one that only takes a path both work.

Run it:

```sh
lattice setup
lattice run build
```

`lattice setup` provisions every declared engine before installing any
workspace's native dependencies, so a package manager that shells out to a
provisioned compiler sees it on `PATH` during install. `lattice run` also
provisions on demand if you skip `setup` — a task never runs against a
missing or wrong-versioned toolchain.

What you see the first time: a provisioning line for the engine, then the
task itself.

```text
lattice: provisioning engine 'node' via installCmd into .lattice/toolchains/node/tmp-a1b2c3d4
lattice: running `build` across 1 workspace(s)
```

What you see every time after: nothing — an existing, verified install is
reused. If the install command itself fails, the run stops with its output
inline:

```text
Error: engine 'node': installCmd failed:
<installer's stderr>
```

If the install succeeds but the result still does not satisfy the version
constraint, that is also a hard failure:

```text
Error: engine 'node' provisioned 19.8.1 does not satisfy '>=20.0.0'
```

Tell a teammate whose machine fails this check: nothing on their machine
needs to change. The install command runs the same way for them; if it fails,
the fix is in the `installCmd` itself (network access, a broken URL, a
platform-specific installer), not in anything local to their environment.

### Where the provisioned tool lands

Every provisioned engine goes under `.lattice/toolchains/<engine>/`, inside
the repo:

```text
.lattice/toolchains/node/20.11.1-a1b2c3d4/
  bin/          prepended to the task's PATH
  pins.json     { "engine", "version", "installHash", "bin" }
```

The version-and-hash directory name (`20.11.1-a1b2c3d4`) is
content-addressed: the hash is the first 8 hex characters of
`sha256(installCmd)`. Change the `installCmd` and Lattice provisions into a
new directory instead of mutating the old one.

Nothing is written outside the repo — no shell profile, no global package
manager state. `rm -rf .lattice` removes every provisioned toolchain along
with the cache; the next `lattice run` or `lattice setup` reprovisions from
`installCmd` from scratch. This is also the fix when a provisioned tool ends
up in a bad state you don't want to debug.

## Different workspaces need different versions

Declare `engines` inside a workspace instead of (or in addition to) the root.
A workspace's `engines` entries override the root's, key by key — everything
else in the root map still applies:

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

Here `legacy-api` runs against whatever `node` on `PATH` satisfies `>=18.0.0`,
and `web` requires `>=20.0.0` — both checked against the same `PATH`, so this
only works if one host `node` binary can satisfy both, or if you swap in
`installCmd` per workspace so each gets its own provisioned copy:

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

Because the provisioned directory is content-addressed by the resolved
`installCmd`, two workspaces asking for the same engine name with different
install commands provision into two separate directories under
`.lattice/toolchains/node/`, and each task only gets its own workspace's `bin`
dir on `PATH`. Verify both resolve independently:

```sh
lattice run build --filter legacy-api
lattice run build --filter web
```

## My tool isn't one Lattice knows

If your engine's name is not in the well-known list above, the bare string
form is rejected — Lattice has no built-in rule for checking its version.
Loading a config like this fails immediately:

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

Use the object form and tell Lattice how to ask the tool its own version:

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

This is validate-only, the same as a well-known engine with a bare string —
add `installCmd` to have Lattice provision it instead, exactly as in
["I want Lattice to install it"](#i-want-lattice-to-install-it) above.
Lattice parses whatever number-looking substring `versionCmd` prints (`v2.6.7`,
`alpes 2.6.7 (build 41)`, `2.6`) and compares it against the constraint, so
`versionCmd` only needs to print a version somewhere in its output.

## Removing a provisioned toolchain entirely

`.lattice/toolchains` and `.lattice/cache` are both regenerable and both
excluded from version control (`lattice init` writes them into
`.gitignore`). Deleting `.lattice` is a complete, safe reset of every
provisioned tool this repo has ever installed:

```sh
rm -rf .lattice
lattice run build
```

The next run reprovisions each declared `installCmd` from scratch and
rebuilds the cache. Nothing outside the repo is touched, so this never
affects a tool installed globally on the machine or any other project.
