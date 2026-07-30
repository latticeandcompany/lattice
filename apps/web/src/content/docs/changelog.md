---
title: Changelog
description: Release history for Lattice, curated from the repo's CHANGELOG.md.
group: Reference
order: 9
---

# Changelog

This page curates the user-facing history from the repo's own `CHANGELOG.md`,
newest first. That file is the source of record; this page drops the entries
that only matter to someone working inside the repo and links out to the
relevant docs where they exist.

Lattice is currently at `1.0.0-beta-2` — a pre-release build; there is no
`1.0.0` yet. Versions follow semver: a major bump means a breaking change to
the `lattice.json` schema or the CLI surface. A version bump is also a full
cache miss, because the running version is one of the inputs hashed into every
task's cache key, so the first run after an upgrade re-runs everything. See
[Upgrading](/lattice/docs/upgrading) and [Caching](/lattice/docs/caching).

## Flags for what used to be environment variables — 2026-07-29

Four settings that could only be given through the environment are now flags.
`--theme light|dark` picks the splash art's teal shade, and `--release-base-url`
sets where release archives are downloaded from — both global, so they parse on
`lattice` and on every subcommand. `--release-latest-url` and
`--release-list-url` sit on `lattice upgrade`, which is the only command that
resolves `latest`.

The matching `LATTICE_*` variables all still work; the flag wins where both are
given. Two things stay variables on purpose: `LATTICE_SWITCHED_FROM`, which is
read by a *different build* of Lattice after a version switch and so cannot be a
flag that build might not know, and `LATTICE_TOOLCHAIN_DIR`, which Lattice hands
to your `installCmd` rather than reads. See
[Environment variables](/lattice/docs/environment-variables) and the
[CLI reference](/lattice/docs/cli).

## One color per task in the plain stream — 2026-07-29

The `workspace:task` label leading each line of the plain stream now carries its
own color, so the interleaved output of a parallel run can be followed one task
at a time. Both halves of the label count: `web:build`, `web:test`, and
`api:build` are three different colors, and the first eight distinct labels in a
run never share one.

Color now follows the terminal rather than the mode, so `-l` at a shell paints
labels while the same run piped, redirected, or under `CI` emits nothing to
strip. `NO_COLOR` still turns it all off. See
[Output and logging](/lattice/docs/output-modes).

## A run that executes nothing says so — 2026-07-29

A run where every task came back from cache now ends with a `FULL CACHE` line
under the summary, so "nothing ran" is one thing to look at instead of a count
to compare against another count. Plain output carries the same line without
color, greppable in a CI log. See
[Output and logging](/lattice/docs/output-modes) and
[Caching](/lattice/docs/caching).

## The toolchain table, filled in — 2026-07-29

CocoaPods, pip, NuGet, and Kotlin are supported end to end, `deno` and `bun` are
runtimes as well as the roles they already had, and the gaps between the driver
table and the engine list are closed. See [Toolchains](/lattice/docs/toolchains)
and [Driver detection](/lattice/docs/drivers).

### Four tools that were only half-known
- `pod` had a driver row but no engine rule and no dependency installer; `pip`
  had an installer no driver could reach; `nuget` and `kotlin` were missing
  outright. All four are now drivers, well-known engines, and known to
  `lattice setup`
- `pip` and `kotlin` have no fingerprint on purpose. A `requirements.txt` is
  read by pip, uv, and pip-tools alike, and a Kotlin project is driven by gradle
  or maven, so both are selected by declaring them in `engines` rather than by
  guessing from a file that names no tool
- `nuget` fingerprints only the legacy `packages.config` layout. A
  `packages.lock.json` counts toward cache keys but is not driver evidence: an
  SDK-style project can carry one and still be a `dotnet` workspace

### One table for engines instead of two that disagreed
- `uv`, `poetry`, `just`, `turbo`, `nx`, `swift`, `dart`, `composer`, `mix`,
  `stack`, `cabal`, `pdm`, and `pipenv` each had a built-in version command but
  were rejected in the string form, so `"engines": { "uv": ">=0.5" }` failed to
  load for no good reason. Every built-in driver is now a well-known engine
- `"python3"` was version-checked by running `python --version`, which on many
  machines is a different interpreter. It runs `python3 --version`

### A driver can fill more than one role
- `deno` is a runtime, a package manager, and a task runner; `bun` is a runtime
  and a package manager; `mix` is a package manager and a task runner. A driver
  competes with its highest-ranked role, so which tool drives a workspace is
  unchanged

### Caching and setup catch up with the table
- 13 more lockfiles feed cache keys — `deno.lock`, `composer.lock`, `mix.lock`,
  `pubspec.lock`, `Package.resolved`, `Podfile.lock`, `packages.lock.json`,
  `pdm.lock`, `Pipfile.lock`, `requirements.txt`, and more. A dependency bump in
  those ecosystems used to come back as a hit
- `lattice setup` installs dependencies for 11 more drivers, so a `.csproj` or
  `Podfile` workspace no longer reports "no known dependency installer" and
  skips
- An ambiguity error over a bare `Cargo.toml`, `go.mod`, `composer.json`,
  `mix.exs`, `pubspec.yaml`, `Package.swift`, `stack.yaml`, `cabal.project`, or
  `.csproj` now names candidate tools instead of listing none

## Installing, upgrading, and running the version a repo pins — 2026-07-28

Lattice can now be installed without a Rust toolchain, and a repo's
`latticeVersion` is enforced rather than merely announced. See
[Installation](/lattice/docs/installation) and
[Upgrading](/lattice/docs/upgrading).

### `curl | sh` installs a target-matched binary into the repo
- The installer detects the OS, architecture and libc, resolves a version,
  downloads the matching release archive, verifies its SHA256 against the
  release's checksums file, and installs `./.lattice/bin/lattice-<version>`
  with `./.lattice/bin/lattice` symlinked to it. Nothing is written outside
  `.lattice`, so `rm -rf .lattice` is the uninstall
- Version resolution, in order: `$LATTICE_VERSION`, then `latticeVersion` from
  `./lattice.json`, then the newest release when the directory has no config
  at all. A `lattice.json` that exists but pins nothing is an error
- It fails loudly, before installing anything, on an unsupported platform, a
  missing pin, a missing release asset, a missing checksums entry, or a digest
  that does not match
- `.lattice/bin/` is now one of the `.gitignore` lines `lattice init` maintains

### Every invocation runs the version the repo pins
- A binary under `.lattice/bin` whose version differs from `latticeVersion`
  now prints one line naming both versions, installs the pinned version if it
  is not already on disk, repoints the symlink, and hands the invocation over
  to it with the arguments untouched. Switching between two branches that pin
  versions you already have is a symlink swap and touches no network
- A binary Lattice did not install — `cargo install`, a distro package, a
  local dev build — is never replaced. It gets an advisory one-line nag
  instead, with a runnable `lattice upgrade <version>`
- `--no-version-check`, `LATTICE_NO_VERSION_CHECK` and
  `settings.versionCheck: false` each skip the whole thing. `upgrade`,
  `version` and `completions` are never handed off: they answer for the
  binary that was invoked, and a completion script has to be the only thing
  on stdout
- A pinned version that cannot be installed is a hard failure naming the
  version and the way past it, rather than silently running a build the repo
  did not ask for

### `lattice upgrade <version|latest>`
- Installs the version, points `.lattice/bin/lattice` at it, and rewrites
  `latticeVersion`. `latest` resolves the newest release; a bare version pins
  it exactly, with or without a leading `v`
- The config is edited as text, so key order, indentation and the rest of the
  file survive a bump
- Re-running for a version already pinned and installed reports that and
  repoints the symlink — the one case where doing nothing would leave the
  repo on the wrong binary

### Releases are published for six targets
- macOS x86_64/aarch64, Linux x86_64 (gnu and musl), Linux aarch64, and
  Windows x86_64, as `lattice-<version>-<target>.tar.gz` archives carrying the
  binary, the license and completion scripts, alongside one
  `lattice-<version>-checksums.txt` and the installer itself

## Four documented promises the code did not keep — 2026-07-28

Groundwork for the first tagged release: statements in the README, the docs,
or a manifest that the code contradicted.

- The minimum Rust version is `1.86`, not the `1.75` every doc previously
  claimed — the real floor, once the lockfile is resolved against each
  dependency's own requirement, comes from `clap`, `sha2`, `indexmap`, and the
  ICU crates reached through `jsonschema`
- `lattice version --json` now reports a real target triple
  (`aarch64-apple-darwin`) in its `target` field instead of a bare
  architecture (`aarch64`); the bare architecture moved to its own `arch`
  field rather than being dropped. See [CLI reference](/lattice/docs/cli)
- Windows is not supported: `lattice-workspace`'s toolchain probe hardcodes a
  Unix shell and a Unix `PATH` separator, so engine version checks and
  toolchain provisioning cannot work there. The docs now say macOS and Linux,
  and point Windows users at WSL2. See
  [Installation](/lattice/docs/installation)

## Long durations print as a clock — 2026-07-28

- Task and run times over a minute now read `4:07` and `1:12:30` instead of
  `247.00s` and `4350.00s`. Under a minute is unchanged (`1.23s`). Applies
  everywhere Lattice prints a duration: per-task completion lines and the run
  summary, in both the interactive and CI reporters. See
  [Output and logging](/lattice/docs/output-modes)

## Nested repos: docs, worked example, and tests — 2026-07-28

- A subtree that already has its own task runner can be declared as a manual
  workspace whose `scripts` shell out to that runner — ordering, `dependsOn`,
  caching as one opaque unit, and validation all fall out of the existing
  workspace mechanism, with no separate feature needed
- Two limitations: a manual workspace must declare any task invoked directly,
  and a downstream workspace must not copy an upstream artifact at build
  time, because a cache key covers only the inputs its own workspace declares
- `examples/nested-repo` ships as a runnable worked example: a JS monorepo
  (npm workspaces, two packages, an inner dependency edge) as one Lattice
  node, plus a downstream service. See [Nested repos](/lattice/docs/nested-repos)

## Docs site search — 2026-07-28

- Full-text search over the documentation, opened with `⌘K`/`Ctrl-K` or `/`,
  navigable with the arrow keys, listing heading-level matches beneath each
  page so a long page points at the section that matched

## Persistent tasks stream their output by default — 2026-07-27

- A `lattice run` that pulls in a persistent task — a dev server, a watcher,
  or anything in its dependency closure — now defaults to raw line-by-line
  output instead of the live TUI, so the process's streaming output stays
  visible; this previously required `-l` (`--loquacious`)
- Non-persistent runs on a terminal still get the interactive TUI
- A persistent task's output always streams live even in raw mode, while
  other per-task output stays collapsed and is surfaced on failure
- Auto-detection no longer fabricates a command for a persistent task: a
  direct-invoke driver (`cargo`, `go`, …) used to invent a command for any
  task name, so `lattice run dev` picked up every Rust and Go workspace as
  `cargo dev` or `go dev` even though no such task exists
- A persistent task now runs only where the workspace declares it, through an
  explicit `scripts` entry or a manifest script for the JS and Deno drivers;
  non-persistent tasks (`build`, `test`, …) still infer as before. See
  [Persistent tasks](/lattice/docs/persistent-tasks) and
  [Output and logging](/lattice/docs/output-modes)

## Stacked commands and a self-healing editor schema — 2026-07-27

- `lattice run` accepts multiple tasks in one invocation
  (`lattice run lint test build`); the roots merge into a single dependency
  graph, so a dependency shared by several roots runs once and independent
  roots parallelize where the graph allows
- All existing flags (`--filter`, `--concurrency`, `--continue`, `--dry-run`,
  `--no-cache`) apply to the combined run, and an unknown task in the list
  fails fast and names the offender
- `--sequentially` / `-s` runs each task's graph to completion in the order
  given before starting the next; fail-fast stops at the first failed phase,
  and `--continue` runs the remaining phases and still exits non-zero
- `run`, `setup`, and `prune` write `.lattice/schema.json` when it is
  missing, as happens with a cleared cache directory or a clone where it was
  never committed, so an editor's JSON language server can resolve the
  config's `$schema`. An existing copy is left untouched to avoid churn. See
  [Task graph](/lattice/docs/task-graph) and [CLI reference](/lattice/docs/cli)

## The documented install command installed the wrong software — 2026-07-27

- The docs site told readers to run `cargo install lattice`, but `lattice` on
  crates.io is an unrelated markdown linter, so anyone following the
  getting-started page or the landing-page copy button got someone else's
  tool. Both were corrected to a working install path. See
  [Installation](/lattice/docs/installation) for the current instructions

## License of record was inconsistent — 2026-07-27

- `LICENSE` is ISC while the workspace manifest declared `license = "MIT"`;
  the manifest now says ISC
