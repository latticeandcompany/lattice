---
title: Installation
description: Install Lattice, verify it, add shell completions, and remove it.
group: Overview
order: 2
---

# Installation

Lattice is a single binary. The install script puts it in `.lattice/bin/` inside
the repo you install it into.

## Install with the script

From the root of the repo you want to use Lattice in:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

The script downloads the archive for your platform, checks its SHA-256 against
the release's published checksums file, and refuses to install on a mismatch.
The binary lands at `.lattice/bin/lattice-<version>`, with
`.lattice/bin/lattice` symlinked to it.

Which version it installs, in order:

1. `$LATTICE_VERSION`, if that variable is set.
2. `latticeVersion` from `./lattice.json`, if the file exists. A `lattice.json`
   with no `latticeVersion` is an error, not a fallback.
3. The newest published release, if the directory has no `lattice.json`.

To install a specific version, set `LATTICE_VERSION`:

```sh
LATTICE_VERSION=0.2.0 curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

The script also appends a `PATH` line to your shell config, so that a bare
`lattice` in this repo resolves to `.lattice/bin/lattice`. It picks the file
from `$SHELL`: `.zshrc`, `.bashrc` or `.bash_profile`, or
`fish/config.fish`. To skip that edit, pass `--no-modify-path` or set
`LATTICE_NO_PATH=1`:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

Then invoke the binary by its relative path:

```sh
./.lattice/bin/lattice run build
```

If the repo has a `.gitignore` that does not list `.lattice/bin/`, the script
appends it. The binaries there are machine-local.

Once `lattice.json` pins a `latticeVersion`, every later invocation of the
installed binary reads that pin and switches to it if it differs, so a branch
that bumps the pin takes effect on the next command with no re-install step.
See [Upgrading](/lattice/docs/upgrading).

## Supported platforms

| Platform | Target triple |
| --- | --- |
| macOS, Apple silicon | `aarch64-apple-darwin` |
| macOS, Intel | `x86_64-apple-darwin` |
| Linux, x86_64 (glibc) | `x86_64-unknown-linux-gnu` |
| Linux, x86_64 (musl) | `x86_64-unknown-linux-musl` |
| Linux, aarch64 (glibc) | `aarch64-unknown-linux-gnu` |
| Windows, x86_64 (MSVC) | `x86_64-pc-windows-msvc` |

aarch64 Linux is published for glibc only. On an aarch64-musl host, build from
source.

The install script needs a POSIX shell. On Windows, run it inside WSL2, where
`uname` reports Linux and the script installs the matching Linux archive.
Native Windows has no scripted installer: download the
`x86_64-pc-windows-msvc` archive from the release page, extract `lattice.exe`,
and put it on `PATH`.

## Build from source

Building from source needs Rust 1.86 or newer, the workspace `rust-version`.
Lattice is not published to crates.io, so `cargo install lattice` will not find
it. Install from the repository:

```sh
cargo install --git https://github.com/latticeandcompany/lattice lattice
```

Or clone and build it:

```sh
git clone https://github.com/latticeandcompany/lattice
cd lattice
cargo build --release
```

The binary is at `target/release/lattice`. The version-pin switch described
above never touches a binary built this way. It only replaces files it put in a
repo's own `.lattice/bin/`.

## Verify the install

```sh
lattice --version
```

```text
lattice 1.0.0-beta-2
```

`lattice version` prints the same version under the mark. For a
machine-readable line, add `--json`:

```sh
lattice version --json
```

```json
{"version":"1.0.0-beta-2","target":"aarch64-apple-darwin","arch":"aarch64"}
```

Running `lattice` with no arguments prints the mark and points you at `--help`.

## Add shell completions

`lattice completions <shell>` prints a completion script to stdout for `bash`,
`elvish`, `fish`, `powershell`, or `zsh`.

```sh
lattice completions zsh
```

Load the script the way your shell expects:

```sh
# bash: evaluate it on shell startup
echo 'source <(lattice completions bash)' >> ~/.bashrc

# zsh: a directory on $fpath, loaded before compinit runs
lattice completions zsh > "${fpath[1]}/_lattice"

# fish: autoloaded, no sourcing needed
lattice completions fish > ~/.config/fish/completions/lattice.fish

# PowerShell: append to your profile
lattice completions powershell >> $PROFILE

# elvish: evaluate it from rc.elv
echo 'eval (lattice completions elvish | slurp)' >> ~/.config/elvish/rc.elv
```

Regenerate the script after every upgrade. A completion script comes from the
command tree of the binary that produced it, so an out-of-date one omits flags
a newer `lattice` added.

## Remove Lattice

Everything Lattice installs for itself lives under one directory at the repo
root:

```text
.lattice/
  bin/         lattice-<version> binaries and the lattice symlink (gitignored)
  cache/       task result cache (gitignored)
  toolchains/  provisioned engine versions (gitignored)
  schema.json  the lattice.json JSON Schema (committed)
```

To remove every binary, cached result, and provisioned toolchain:

```sh
rm -rf .lattice
```

The one file outside the repo is the shell config the install script appended
its `PATH` line to. Delete that line by hand. The script names the file it
edited in its own output.

## Next

[Getting started](/lattice/docs/getting-started) writes a `lattice.json` and
runs a first cached task. To move a repo between versions, see
[Upgrading](/lattice/docs/upgrading).
