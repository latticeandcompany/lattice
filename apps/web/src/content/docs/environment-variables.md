---
title: Environment variables
description: Every variable Lattice reads, every one it sets for a task, and how a task's own env list resolves.
group: Reference
order: 4
---

# Environment variables

Most of what Lattice reads from the environment also has a flag; where both are
given, the flag wins (see
[precedence](/lattice/docs/cli#option-precedence)). Lattice also sets a few
variables in the environment of the commands it spawns.

## What Lattice reads

| Variable | Flag | Effect | Counts as set when |
| --- | --- | --- | --- |
| `CI` | `-l`/`--loquacious` | Forces [`Raw` output](/lattice/docs/output-modes) instead of the interactive TUI. | Present, any value, including empty |
| `NO_COLOR` | — | Disables ANSI color. Nothing in `lattice.json` or on the CLI turns color back on once this is set. | Present, any value, including empty |
| `LATTICE_NO_VERSION_CHECK` | `--no-version-check` | Suppresses both the version-drift nag and the automatic switch-over to a pinned `latticeVersion`. `settings.versionCheck: false` does the same for the whole repo — see [Upgrading](/lattice/docs/upgrading). | Present, any value, including empty |
| `LATTICE_SWITCHED_FROM` | — | Internal. Set by Lattice on the process it hands an invocation to after a version switch, so that process doesn't switch again. Not meant to be set by hand. | Present, any value |
| `LATTICE_THEME` | `--theme` | `light` or `dark` (case-insensitive); forces the teal shade used in the splash/logo art. Any other value is ignored. | Recognized value present |
| `COLORFGBG` | `--theme` | Splash theme only, read as `fg;bg` (or `fg;...;bg`): a trailing `7` or `15` is a light background, anything else dark. | Neither `--theme` nor `LATTICE_THEME` gives a recognized value, and this parses |
| `LATTICE_RELEASE_BASE_URL` | `--release-base-url` | Base URL that `lattice upgrade` and the automatic version switch download release archives from. A `file://` base needs no network. | Present and not empty/whitespace-only |
| `LATTICE_RELEASE_LATEST_URL` | `--release-latest-url` | Endpoint that resolves `lattice upgrade latest` to the newest *stable* release. | Present and not empty/whitespace-only |
| `LATTICE_RELEASE_LIST_URL` | `--release-list-url` | Endpoint used as a fallback when the latest-stable endpoint has nothing to name (every release so far is a pre-release). | Present and not empty/whitespace-only |

`--theme` and `--release-base-url` are global: they parse on `lattice` itself
and on every subcommand. `--release-latest-url` and `--release-list-url` live
on `lattice upgrade`, the only command that asks those endpoints anything.

Presence is the whole test for `CI`, `NO_COLOR`, `LATTICE_NO_VERSION_CHECK`,
and `LATTICE_SWITCHED_FROM` — `CI=` counts the same as `CI=1` or `CI=false`.
The three `LATTICE_RELEASE_*_URL` overrides instead treat an empty or
whitespace-only value as unset and fall back to the default, so an inherited
`LATTICE_RELEASE_BASE_URL=` doesn't break the default download path. A blank
value passed to the matching flag is treated the same way.

`CI` and `-l`/`--loquacious` are independent triggers of the same `Raw` mode:
neither overrides the other, and nothing forces interactive mode back on from
inside a `CI=1` environment. See [Output and
logging](/lattice/docs/output-modes) for the rest of how output mode is chosen.

## Across a version switch

A repo that pins `latticeVersion` hands the invocation to the pinned build,
passing your command line through unchanged and setting `LATTICE_SWITCHED_FROM`
on it so it does not switch again. That build can be older than the flags you
typed, and an unrecognized flag is a parse error.

`LATTICE_SWITCHED_FROM` therefore has no flag: a build too old to know the flag
would reject it and fail the handover, while a variable it does not read is
ignored. For the same reason, exporting `LATTICE_RELEASE_BASE_URL` reaches a
pinned build that predates `--release-base-url`, where passing the flag would
not. See [Upgrading](/lattice/docs/upgrading) for the switch itself and for how
the three version-check suppressions interact.

## What Lattice sets for a spawned command

A task's command runs through the platform shell (`sh -c` on Unix, `cmd /C` on
Windows) as a child process that inherits the full environment Lattice itself
was invoked with — nothing is cleared. On top of that inheritance, Lattice sets:

| Variable | Shape | When |
| --- | --- | --- |
| `PATH` | The resolved toolchain bin directories for that task, in order, prepended ahead of the inherited `PATH`. | Every task with a provisioned engine, and every `lattice setup` install command |
| Each name listed in a task's `env` | The value read from Lattice's own environment at the moment the cache key was computed. | Every task that declares `env` |
| `LATTICE_TOOLCHAIN_DIR` | Absolute path to the toolchain's install directory, both as an environment variable and literal-substituted into the `installCmd` string itself. | Only while running an engine's `installCmd`, never for the task command that follows |

`PATH` is scoped to that one child process — it never touches the shell Lattice
itself is running in. See [Engines and
provisioning](/lattice/docs/engines) for what gets installed into
`LATTICE_TOOLCHAIN_DIR` and how the resulting bin directory ends up on `PATH`.

## How a task's `env` list resolves

`env` on a task is a list of variable *names*, not `NAME=value` pairs:

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

Lattice reads each name from its own process environment — whatever was
exported into the shell you ran `lattice` from. It does not load `.env` files;
a variable defined only in one is unset as far as Lattice is concerned. A name
with no value contributes nothing, the same as if it weren't listed.

The resolved `(name, value)` pairs are sorted by name and hashed into the
task's cache key alongside its command, its input files, and its resolved
toolchain identity — see [Caching](/lattice/docs/caching) for the rest of what
feeds that key. Lattice sets those same values on the task's own process, on
top of the full inheritance above.

Listing a variable ties the cache key to its value, so a build that behaves
differently under a new value re-runs instead of restoring a result computed
under the old one. Leaving it out does not hide it from the task — inheritance
does not care about `env` — but the key no longer moves when the value changes,
and a command whose output depends on an undeclared variable can be served a
stale hit from a run made under a different value. Declare what the command's
result depends on, nothing more: a variable that is noisy but irrelevant (a
timestamp, a machine-specific temp path) costs cache hits.

## Repo-wide `globalEnv`

A variable that changes what *every* task produces belongs in the root-level
`globalEnv` rather than repeated in each task's `env`:

```json
{
  "globalEnv": ["NODE_ENV", "CI"]
}
```

The names resolve the same way and are hashed into every task's key. The one
difference is that `globalEnv` names are not set on the task's process the way
`env` names are — they are already in the environment Lattice inherited, so
there is nothing to re-apply. A task's own `env` list still applies on top.
