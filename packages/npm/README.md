# @latticeandcompany/lattice

**A high-performance, local toolchain for managing monorepos.**

Lattice runs the tasks in your repo in dependency order and in parallel. It stores
each result under a hash of everything that produced it, so a task whose inputs
have not changed does not run again.

It runs each workspace with the tool that workspace already uses, and pins that
tool's version so every machine builds against the same one.

Lattice is a single Rust binary. This package is a wrapper that installs that
binary through npm instead of through a shell script.

## Install

```sh
npm install --save-dev @latticeandcompany/lattice
```

`pnpm add -D`, `yarn add -D`, and `bun add -d` take the same package name. Run
Lattice from a package script, or with `npx`:

```sh
npx lattice run build
```

This package is a wrapper around the real binary. It passes everything after the
command name straight through, so `npx lattice` and an installed `lattice` are
the same program with the same help.

## Only one binary is installed

The binary is not in this package. Six sibling packages carry one build each, and
this package depends on all six as `optionalDependencies`:

| Package | Platform |
| --- | --- |
| `@latticeandcompany/lattice-darwin-arm64` | macOS, Apple silicon |
| `@latticeandcompany/lattice-darwin-x64` | macOS, Intel |
| `@latticeandcompany/lattice-linux-x64-gnu` | Linux, x86_64 (glibc) |
| `@latticeandcompany/lattice-linux-x64-musl` | Linux, x86_64 (musl) |
| `@latticeandcompany/lattice-linux-arm64-gnu` | Linux, aarch64 (glibc) |
| `@latticeandcompany/lattice-win32-x64-msvc` | Windows, x86_64 (MSVC) |

Each package declares the `os` and `cpu` it is for, so your package manager
unpacks only the one that matches. The musl package covers Alpine and other
distributions without glibc.

The install downloads nothing beyond those packages and runs no `postinstall`
script. It therefore works offline, from a lockfile, behind a proxy, and under
`--ignore-scripts`.

On Windows on ARM, npm installs the x86_64 build, which Windows runs under
emulation. Lattice publishes no build for Linux, aarch64 (musl). Build from
[source](https://latticeandcompany.github.io/lattice/docs/installation) on that
platform.

If Lattice reports that a platform package did not install, the usual cause is an
npm lockfile made on a different platform
([npm/cli#4828](https://github.com/npm/cli/issues/4828)). Delete `node_modules`
and `package-lock.json`, then install again.

## An npm install does not follow the pin

A repo's `lattice.json` pins a `latticeVersion`. A binary that `install.sh` put
under `.lattice/bin` honors that pin by switching to the version named in it. A
binary in `node_modules` does not, because your lockfile has already chosen the
version.

When the two disagree, Lattice runs the installed version and prints the mismatch
on stderr. Update whichever of the two is behind. To silence the warning instead,
set `"settings": { "versionCheck": false }` in `lattice.json`, pass
`--no-version-check`, or set `LATTICE_NO_VERSION_CHECK`.

## Spawn the binary from Node

The package exports the path to the binary, so another tool can spawn it
directly:

```js
import { spawnSync } from 'node:child_process';
import { binaryPath, version } from '@latticeandcompany/lattice';

console.log(version); // the CLI version this package carries
spawnSync(binaryPath(), ['run', 'build'], { stdio: 'inherit' });
```

`require()` gets the same two exports. `binaryPath()` throws when there is no
binary to run. The message says which of the two reasons applies: nothing is
published for this platform, or the package that carries the binary is missing.

## Add shell completions

npm installs the binary, not its completions. Generate them from the binary:

```sh
npx lattice completions zsh > ~/.zsh/completions/_lattice
```

`lattice completions` also writes scripts for bash, elvish, fish, and powershell.

## Install without npm

The installer script puts the binary in `.lattice/bin` inside your repo, uses no
package manager, and honors the `latticeVersion` pin:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

[Installation](https://latticeandcompany.github.io/lattice/docs/installation)
covers Windows, pinning a specific version, and removing Lattice.

## Links

- [Documentation](https://latticeandcompany.github.io/lattice/docs)
- [Source](https://github.com/latticeandcompany/lattice)
- [Issues](https://github.com/latticeandcompany/lattice/issues)

ISC licensed.
