---
title: Pinning tool versions
description: What to write under engines for each intent, from a version check to a full install.
group: Guides
order: 4
---

# Pinning tool versions

Every block below is an `engines` map you can put in `lattice.json`: at the root,
where it applies to every workspace, or in an override, where it changes some
workspaces. The shape decides what Lattice does; the model behind that is
[Engines and provisioning](/lattice/docs/engines).

## Stop the run when the host tool is wrong

A bare constraint checks the tool already on `PATH` and installs nothing:

```json
{ "engines": { "node": ">=22" } }
```

A machine that satisfies it logs nothing. One that does not stops before any
task starts:

```text
error: workspace 'web' cannot run
  engine 'node' does not match the version this repo pins
    wanted   >=22
    found    20.11.1 at /usr/local/bin/node
    ran      node --version
    fix      install node >=22, or give the engine an installCmd so Lattice installs it for this repo
```

A machine without the tool at all gets `is not installed, or not on PATH`. The
bare string works for the [40 well-known
engines](/lattice/docs/toolchains#well-known-engines).

A version the repo already records elsewhere is a good source. `lattice init`
proposes what it finds in `.nvmrc`, `.tool-versions`, `rust-toolchain.toml`,
`.python-version`, `package.json` `packageManager` and `engines`, and `go.mod`.

## Have Lattice install the tool

Add `installCmd`. It runs once, into a directory Lattice controls, and the
result is checked against `version`:

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

`$LATTICE_TOOLCHAIN_DIR` is the empty directory to install into, available as
an environment variable and substituted literally into the command. Put the
executables in its `bin`, or set `bin` to wherever they land.

```sh
lattice setup
lattice run build
```

`setup` installs every engine before any workspace's dependencies, so a package
manager that shells out to an installed compiler finds it. `lattice run`
installs on demand if you skip `setup`. The first run notes the install; later
runs reuse it.

A failing install stops the run and shows why:

```text
error: workspace 'docs' cannot run
  engine 'just' could not be installed: its installCmd exited with code 22
    ran      curl -fsSL https://just.systems/install.sh | bash -s -- --to "$LATTICE_TOOLCHAIN_DIR/bin"
    output   curl: (22) The requested URL returned error: 404
    fix      the command runs in a shell with $LATTICE_TOOLCHAIN_DIR set to an empty directory; it should put the tool there, with its executables under `bin`
```

An install that succeeds with the wrong version fails with `installed a version
that does not match its constraint`, showing what was wanted and found.

### Where it lands

```text
.lattice/toolchains/just/1.30.0-1a2b3c4d/
  bin/          on each task's PATH
  pins.json     the version installed and the hash that produced it
```

The hash is the first 8 hex characters of `sha256(installCmd)`, so changing the
command installs into a new directory.

## Use the pinned tools in your terminal

A pinned tool is on the `PATH` of every task Lattice runs, and nowhere else.
`lattice env` prints that `PATH` for your shell:

```sh
eval "$(lattice env)"          # bash, zsh
lattice env --shell fish | source
```

Run it inside a workspace to include that workspace's pins and the tools its
package manager installed, such as `node_modules/.bin`.

## Give some workspaces their own version

Set `engines` in an override. Its entries win per name, and the rest of the root
map still applies:

```json
{
  "engines": { "node": ">=18" },
  "overrides": {
    "web": { "engines": { "node": ">=22" } }
  }
}
```

Both check the same host `node`, so this works only if one install satisfies
both. To give each its own copy, give each an `installCmd`:

```json
{
  "overrides": {
    "legacy-api": {
      "engines": {
        "node": { "version": "^18", "installCmd": "install-node.sh --version 18 --dest $LATTICE_TOOLCHAIN_DIR" }
      }
    },
    "web": {
      "engines": {
        "node": { "version": "^22", "installCmd": "install-node.sh --version 22 --dest $LATTICE_TOOLCHAIN_DIR" }
      }
    }
  }
}
```

Two install commands install into two directories under
`.lattice/toolchains/node/`, and each task gets only its own workspace's.

## Gradle and Maven wrappers

A Gradle or Maven build usually pins its own version through the wrapper. A
`gradle` engine is checked through that wrapper, run in the workspace:

```json
{ "engines": { "gradle": ">=8.10" } }
```

That checks what `./gradlew --version` reports, so a machine with no `gradle`
installed passes. To pin the JDK the build runs on, pin `java`.

## A tool Lattice does not know

A name outside the well-known list needs a `versionCmd`:

```json
{
  "engines": {
    "alpes": { "version": ">=2.0.0", "versionCmd": "alpes --version" }
  }
}
```

The string form for such a name fails the load, with the object form spelled
out. Add `installCmd` to install it, as above.

## Remove an installed toolchain

```sh
rm -rf .lattice/toolchains
```

The next run reinstalls from each `installCmd`. Nothing outside the repo is
touched.
