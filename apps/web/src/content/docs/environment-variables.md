---
title: Environment variables
description: Every variable Lattice reads, every one it sets for a task, and how a task's own env list resolves.
group: Reference
order: 4
---

# Environment variables

Almost everything Lattice reads from the environment is also a flag. Prefer the
flag: it scopes to the invocation you typed it on, and it wins where both are
given. The variables are still read, so an exported value in a CI job keeps
working.

Lattice also sets a small set of its own in the environment of the commands it
spawns. This page is the exhaustive list of both, plus how a task's declared
`env` names turn into resolved values and a cache-key input.

## What Lattice reads

| Variable | Flag to prefer | What it does | Counts as set when |
| --- | --- | --- | --- |
| `CI` | `-l`/`--loquacious` | Forces [`Raw` output](/lattice/docs/output-modes) instead of the interactive TUI. | Present, any value, including empty |
| `NO_COLOR` | — | Disables ANSI color in output. Nothing in `lattice.json` or on the CLI turns color back on once this is set. | Present, any value, including empty |
| `LATTICE_NO_VERSION_CHECK` | `--no-version-check` | Suppresses both the version-drift nag and the automatic switch-over to a pinned `latticeVersion`. `settings.versionCheck: false` does the same for the whole repo — see [Upgrading](/lattice/docs/upgrading) for how the three interact. | Present, any value, including empty |
| `LATTICE_SWITCHED_FROM` | — | Internal. Set by Lattice on the process it hands an invocation to after a version switch, so that process doesn't try to switch again. Not meant to be set by hand. | Present, any value |
| `LATTICE_THEME` | `--theme` | `light` or `dark` (case-insensitive), forces the teal shade used in the splash/logo art. Any other value is ignored. | Recognized value present |
| `COLORFGBG` | `--theme` | Consulted for the splash theme only when neither `--theme` nor `LATTICE_THEME` is set. Read as `fg;bg` (or `fg;...;bg`); a trailing `7` or `15` is treated as a light background, anything else as dark. | Neither of those set and this parses |
| `LATTICE_RELEASE_BASE_URL` | `--release-base-url` | The base URL `lattice upgrade` and the automatic version switch download release archives from. A `file://` base makes the install path work without a network. | Present and not empty/whitespace-only |
| `LATTICE_RELEASE_LATEST_URL` | `--release-latest-url` | The endpoint used to resolve `lattice upgrade latest`'s newest *stable* release. | Present and not empty/whitespace-only |
| `LATTICE_RELEASE_LIST_URL` | `--release-list-url` | The endpoint used as a fallback when the latest-stable endpoint has nothing to name (every release so far is a pre-release). | Present and not empty/whitespace-only |

`--theme` and `--release-base-url` are global: they parse on `lattice` itself
and on every subcommand, before or after the subcommand name.
`--release-latest-url` and `--release-list-url` live on `lattice upgrade`,
because resolving `latest` is the only thing that asks those endpoints
anything.

Two things worth noting about that "counts as set" column. `CI`,
`NO_COLOR`, `LATTICE_NO_VERSION_CHECK`, and `LATTICE_SWITCHED_FROM` are all
checked with presence alone — `CI=` (empty string) counts exactly the same as
`CI=1` or `CI=false`. The three `LATTICE_RELEASE_*_URL` overrides are the one
exception: an empty or whitespace-only value is treated as unset and falls
back to the default, so `LATTICE_RELEASE_BASE_URL=` in an inherited
environment doesn't silently break the default download path. The same is true
of a blank value passed to the matching flag.

## The two that stay variables

`LATTICE_SWITCHED_FROM` has no flag, and that is deliberate. It is how the
process handling a version switch tells the process it hands the invocation to
not to switch again. That second process is a *different build* of Lattice, and
one older than the flag would reject it as unrecognized and fail the handover.
An environment variable is simply ignored by a build that does not read it.

The same reasoning gives the release URL variables one job the flags can't
do. The handover passes your whole command line through to the pinned build, so
if a repo pins a version older than these flags, `--release-base-url` reaches
that older binary as a parse error. Exporting `LATTICE_RELEASE_BASE_URL`
instead works across every version.

`CI` and `-l`/`--loquacious` are independent triggers for the same `Raw`
mode — neither overrides the other, and there's no way to force interactive
mode back on from inside a `CI=1` environment. See
[Output and logging](/lattice/docs/output-modes) for the rest of how output
mode is chosen, and [Upgrading](/lattice/docs/upgrading) for the full
precedence story of the version-check suppressions.

## What Lattice sets for a spawned command

A task's command runs through the platform shell (`sh -c` on Unix, `cmd /C`
on Windows) as a child process that inherits the full environment Lattice
itself was invoked with — nothing is cleared. On top of that inheritance,
Lattice explicitly sets:

| Variable | Shape | When |
| --- | --- | --- |
| `PATH` | The resolved toolchain bin directories for that task, in order, prepended ahead of the inherited `PATH`. | Every task with a provisioned engine, and every `lattice setup` install command |
| Each name listed in a task's `env` | The value read from Lattice's own environment at the moment the cache key was computed. | Every task that declares `env` |
| `LATTICE_TOOLCHAIN_DIR` | Absolute path to the toolchain's install directory, both as an env var and literal-substituted into the `installCmd` string itself. This one has no flag equivalent and never will: it is something Lattice hands *to* your `installCmd`, not something you tell Lattice. | Only while running an engine's `installCmd`, never for the task command that follows |

`PATH` is scoped to that one child process — it never touches the shell
Lattice itself is running in. See [Engines and provisioning](/lattice/docs/engines)
for what gets installed into `LATTICE_TOOLCHAIN_DIR` and how the resulting
bin directory ends up on `PATH`.

## How a task's `env` list resolves

`env` on a `PipelineTask` is a list of variable *names*, not `NAME=value`
pairs:

```json
{
  "tasks": {
    "build": {
      "inputs": ["src/**/*"],
      "env": ["NODE_ENV", "API_BASE_URL"]
    }
  }
}
```

For each name, Lattice reads the value from its own process environment —
whatever was exported into the shell you ran `lattice` from. It does not load
`.env` files; if a variable is only defined in one, it is unset as far as
Lattice is concerned. A name with no value at all in the environment
contributes nothing — the same as if it weren't listed. Only a variable that
is actually set changes anything.

The resolved `(name, value)` pairs are sorted by name and hashed into the
task's cache key alongside its command, its input files, and its resolved
toolchain identity — see [Caching](/lattice/docs/caching) for the rest of
what feeds that key. The same resolved values are also what Lattice sets on
the task's own process, on top of the full inheritance described above.

That leaves a practical difference between listing a variable and not:

- **List it**, and a different value on a later run changes the cache key,
  so a build that behaves differently under a new value re-runs instead of
  restoring a result computed under the old one.
- **Don't list it**, and the task's process still sees it — inheritance
  doesn't care about `env` — but the cache key doesn't move when it changes.
  A command whose output actually depends on an undeclared variable can be
  served a stale cache hit from a run made under a different value, with no
  indication anything was skipped.

The rule is the same one that applies to `inputs`: declare what the command's
result actually depends on, nothing more. Listing a variable that's noisy but
irrelevant (a timestamp, a machine-specific temp path) costs you cache hits
you didn't need to lose.
