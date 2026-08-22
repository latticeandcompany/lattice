---
title: Installation
description: Install Lattice, verify it, add shell completions, and remove it.
group: Overview
order: 2
---

# Installation

Lattice is a single binary. The installer puts it in `.lattice/bin/` inside the
repo you install it into.

## Install with the script

From the root of the repo you want to use Lattice in:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

This runs anywhere there is a POSIX shell, Git Bash and WSL2 included. In
PowerShell, run [`install.ps1`](#install-on-windows) instead.

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

## Install on Windows

Windows has two kinds of shell, and an installer for each.

In PowerShell:

```powershell
irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
```

In Git Bash, MSYS2, or Cygwin, use `install.sh`. It recognizes those
environments and installs the Windows binary rather than the Linux one:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh
```

Either one checks the download against the release's published checksums, then
writes `.lattice\bin\lattice-<version>.exe` and puts a copy of it at
`.lattice\bin\lattice.exe`. Windows withholds the privilege a symlink needs, so
the stable path is a copy rather than a link. `lattice upgrade` replaces it the
same way.

`install.ps1` needs `tar.exe`, which ships with Windows 10 1803 and later. On an
older build, install Git for Windows and run `install.sh` from Git Bash.

Which version it installs is resolved exactly as above, with
`$env:LATTICE_VERSION` standing in for `$LATTICE_VERSION`:

```powershell
$env:LATTICE_VERSION = '0.2.0'
irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
```

### PATH on Windows

`install.ps1` adds `.lattice\bin` to your user `PATH`, so PowerShell, `cmd`, and
anything else you open afterwards resolve a bare `lattice`. On a terminal it
asks before it writes:

```text
Add C:\src\myrepo\.lattice\bin to your user PATH? [Y/n]:
```

Decline and it prints the one line that puts the directory on `PATH` for the
current session instead. A non-interactive run has no one to ask, so it skips
the edit unless you pass `-AssumeYes` or set `$env:LATTICE_ASSUME_YES = '1'`.
Either way, open a new shell before the change takes effect.

To skip the edit outright, set `$env:LATTICE_NO_PATH = '1'`. The `-NoModifyPath`
switch does the same, but `irm | iex` cannot forward arguments to the script it
runs, so a switch has to go through a script block:

```powershell
& ([scriptblock]::Create((irm https://latticeandcompany.github.io/lattice/install.ps1))) -NoModifyPath
```

`install.sh` under Git Bash appends its `PATH` line to `~/.bashrc`, which only
Git Bash reads. PowerShell and `cmd` never see it. Run `install.ps1` if you want
the Windows user `PATH` set.

### WSL2 gets you the Linux binary

Inside WSL2, `uname` reports Linux and `install.sh` installs the Linux archive.
That is the right binary for a repo you build from WSL2, and it is not a Windows
install. Nothing on the Windows side can run it. If you also want `lattice` in
PowerShell, install there too.

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

No `aarch64-pc-windows-msvc` build is published. On Windows on ARM, both
installers take the x64 build, which Windows runs under emulation, and say so
while they do it.

## Build from source

Building from source needs Rust 1.88 or newer, the workspace `rust-version`.
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

On Windows those binaries carry an `.exe` extension and `lattice.exe` is a copy
rather than a symlink, as above.

To remove every binary, cached result, and provisioned toolchain:

```sh
rm -rf .lattice
```

```powershell
Remove-Item -Recurse -Force .lattice
```

The one thing outside the repo is the `PATH` entry. `install.sh` appends a line
to a shell config and names the file it edited in its own output, so delete that
line by hand. `install.ps1` writes your user `PATH` instead: remove the
`.lattice\bin` entry from it, under Environment Variables or with
`[Environment]::SetEnvironmentVariable('Path', ..., 'User')`.

## Next

[Getting started](/lattice/docs/getting-started) writes a `lattice.json` and
runs a first cached task. To move a repo between versions, see
[Upgrading](/lattice/docs/upgrading).
