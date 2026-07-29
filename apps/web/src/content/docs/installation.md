---
title: Installation
description: Install Lattice, verify it, add shell completions, and uninstall it.
group: Overview
order: 2
---

# Installation

Lattice ships as a single binary. Installing it writes files only inside the
repo you install it into — a `.lattice/bin/` directory holds the binary, and
nothing else on the machine changes.

## Platforms

The install script and the published release archives cover:

| Platform | Target triple |
| --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` |
| macOS, Intel | `x86_64-apple-darwin` |
| Linux, x86_64 (glibc) | `x86_64-unknown-linux-gnu` |
| Linux, x86_64 (musl) | `x86_64-unknown-linux-musl` |
| Linux, aarch64 (glibc) | `aarch64-unknown-linux-gnu` |
| Windows, x86_64 (MSVC) | `x86_64-pc-windows-msvc` |

aarch64 Linux is published only for glibc; an aarch64-musl host has no release
asset and needs a source build (see below).

The install script itself needs a POSIX shell, so on Windows run it inside
WSL2 — under WSL2, `uname` reports Linux and the script installs the matching
Linux archive. Outside WSL2, on native Windows, there is no scripted
installer: download the `x86_64-pc-windows-msvc` archive from the release page
by hand, extract `lattice.exe`, and put it wherever you keep other command-line
tools on `PATH`.

## Install with the script

From the root of the repo you want to use Lattice in:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

This downloads the archive for your platform, verifies its SHA-256 against the
release's published checksums file, and refuses to install on a mismatch. The
binary lands at `.lattice/bin/lattice-<version>`, with `.lattice/bin/lattice`
symlinked to it — nothing is written outside that directory, and no shell
profile is sourced by the install itself beyond the `PATH` line described
below.

Which version it installs, in order:

1. `$LATTICE_VERSION`, if that environment variable is set.
2. `latticeVersion` from `./lattice.json`, if the file exists — a
   `lattice.json` with no `latticeVersion` is an error, not a fallback.
3. The newest published release, if the directory has no `lattice.json` at
   all.

```sh
LATTICE_VERSION=0.2.0 curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

By default the script also appends a `PATH` line to your shell config
(`.zshrc`, `.bashrc`/`.bash_profile`, or `fish/config.fish`, chosen from
`$SHELL`) so a bare `lattice` in this repo resolves to `.lattice/bin/lattice`.
Skip that edit with `--no-modify-path` or `LATTICE_NO_PATH=1`:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

Without it on `PATH`, invoke the binary by its relative path instead:

```sh
./.lattice/bin/lattice run build
```

If `.lattice/bin/` isn't already in the repo's `.gitignore`, the script adds
it — the binaries there are machine-local, never committed.

Once a repo's `lattice.json` pins a `latticeVersion`, every later invocation
of the installed binary reads that pin and switches to it if it differs, so a
branch that bumps the pin takes effect on the next command with no
re-install step. See [Upgrading](/lattice/docs/upgrading) for how pinning and
that switch work.

## Building from source

Building from source needs Rust 1.86 or newer (the workspace `rust-version`).
Lattice is not published to crates.io, so a plain `cargo install lattice`
will not find it — install directly from the repository instead:

```sh
cargo install --git https://github.com/latticeandcompany/lattice lattice
```

Or clone and build it yourself:

```sh
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build --release
```

The binary is at `target/release/lattice`. A binary built this way is never
touched by the version-pin switch described above — that only ever replaces
files it put in a repo's own `.lattice/bin/`.

## Verifying the install

```sh
lattice --version
```

```text
lattice 1.0.0-beta-2
```

`lattice version` prints the same version with a branded banner; add `--json`
for a machine-readable line instead:

```sh
lattice version --json
```

```json
{"version":"1.0.0-beta-2","target":"aarch64-apple-darwin","arch":"aarch64"}
```

Running `lattice` with no arguments also prints the version and points you at
`--help`.

## Shell completions

`lattice completions <shell>` prints a completion script to stdout for one of
five shells: `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

```sh
lattice completions zsh
```

Load the script the way each shell expects:

```sh
# bash — evaluate it on shell startup
echo 'source <(lattice completions bash)' >> ~/.bashrc

# zsh — a directory on $fpath, loaded before compinit runs
lattice completions zsh > "${fpath[1]}/_lattice"

# fish — autoloaded, no sourcing needed
lattice completions fish > ~/.config/fish/completions/lattice.fish

# PowerShell — append to your profile
lattice completions powershell >> $PROFILE

# Elvish — evaluate it from rc.elv
echo 'eval (lattice completions elvish | slurp)' >> ~/.config/elvish/rc.elv
```

Regenerate the script after every upgrade: completions are generated from the
exact `clap` command tree of the binary that produced them, so an out-of-date
script can omit flags a newer `lattice` added.

## What's on disk, and uninstalling

Everything Lattice installs for itself lives under one directory at the repo
root:

```text
.lattice/
  bin/         lattice-<version> binaries + the lattice symlink (gitignored)
  cache/       task result cache (gitignored)
  toolchains/  provisioned engine versions (gitignored)
  schema.json  the lattice.json JSON Schema (committed)
```

The only thing that can land outside the repo is the `PATH` line the install
script adds to a shell config, and only when `--no-modify-path` /
`LATTICE_NO_PATH` isn't used.

To remove everything the install script and every later `lattice` invocation
wrote:

```sh
rm -rf .lattice
```

That deletes every installed binary, the cache, and every provisioned
toolchain. If the install script added a `PATH` line to a shell config, delete
that line by hand — the script names the file it edited in its own output, so
that's the one to check.

## Next

Install Lattice, then follow [Getting started](/lattice/docs/getting-started)
to write a `lattice.json` and run your first cached task. To move a repo
between versions afterward, see [Upgrading](/lattice/docs/upgrading).
