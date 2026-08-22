# Security policy

## Report a vulnerability

Report privately. Do not open a public issue.

To report, [open a private security
advisory](https://github.com/latticeandcompany/lattice/security/advisories/new)
on this repo (Security, then Advisories, then Report a vulnerability). The
advisory is private to you and the maintainers.

Include whatever you have:

- What an attacker gains, and what access they need to start
- The affected version (`lattice version --json`) and the platform
- The `lattice.json` and the repo layout needed to reproduce
- Exact commands, and the observed behavior against the expected behavior
- A proof of concept, if you have one

## What to expect

Lattice is maintained by one person, unpaid. Reports are acknowledged as soon as
reasonably possible, typically within a few days, and triaged in order of
severity.

There is **no guaranteed response, fix, or disclosure timeline**. Serious,
clearly exploitable issues get attention first.

We will tell you when a fix lands, and credit you in the release notes and the
advisory unless you ask us not to. Give us a reasonable chance to ship a fix
before you publish.

There is no bug bounty.

## Supported versions

Lattice is pre-1.0. Only the latest release and the tip of `mega` are supported.
Fixes are not backported.

| Version | Supported |
|---|---|
| Latest release, or `mega` | Yes |
| Anything older | No. Upgrade |

## Threat model

Lattice is a build tool. It runs commands on your machine on purpose, and that
shapes what does and does not count as a vulnerability.

### Intended behavior, not a vulnerability

- Executing commands declared in `lattice.json`. Task scripts are shell commands
  and run as you.
- Running the tools it detects. An `auto` workspace resolves to your real package
  manager or build tool and invokes it. Detection acting on a lockfile or a
  declaration file is intended.
- Running a declared `installCmd` to provision a toolchain. Engine provisioning
  executes the install command *from your configuration*. Lattice ships no
  built-in per-language downloaders.
- Reading and writing `./.lattice/`. Toolchains, pins, and the cache live there.

Opening a repo and running Lattice means trusting that repo's `lattice.json` and
its workspaces, exactly as running `npm install`, `make`, or `cargo build` in a
cloned repo does. Treat a `lattice.json` from an untrusted source as untrusted
code, and read it before you run it.

A report that amounts to "a malicious `lattice.json` can run arbitrary commands"
describes the design and will be closed.

### In scope, please report

- Cache poisoning or key collisions: anything that produces a cache hit when a
  declared input, command, environment value, or lockfile changed, or that lets
  one workspace's task restore another's artifacts
- Path traversal on cache restore: a crafted archive entry writing outside the
  workspace or outside `./.lattice/`
- Escape from `./.lattice/`: provisioning, pinning, or pruning that writes to
  global paths, `$HOME`, or arbitrary locations
- Integrity failures in distribution: a downloaded artifact accepted without
  matching its published checksum, an unverified redirect, or a version resolved
  to something other than what `latticeVersion` pins
- Privilege escalation: symlink or temp-file races, world-writable artifacts, or
  unsafe permissions on anything Lattice creates
- Leaking secrets: environment values, tokens, or credentials written into cache
  metadata, logs, or task output that gets stored and replayed
- Command injection through a path that should be inert: a workspace name, path,
  or task name that escapes quoting where no command was supposed to run, for
  example under `--dry-run`
- Denial of service against a developer machine: unbounded extraction, symlink
  loops, or pruning that deletes outside the cache

If you are unsure which side of the line something falls on, report it privately.

## Disclosure

Fixes ship in a normal release with an advisory describing the issue, the
affected versions, and the mitigation. Advisories are published after a fix is
available.
