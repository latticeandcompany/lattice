---
title: Continuous integration
description: Running lattice on a build machine, caching between runs, and reading exit codes.
group: Guides
order: 6
---

# Continuous integration

Running Lattice in CI is the same `lattice setup` and `lattice run` you use on
your own machine. What a build machine does differently is the output it
produces, and that its working directory doesn't survive to the next run.

## Installing lattice in a job

A job starts with no `lattice` binary, so the first step installs one. The
installer is the same one-liner from [Installation](/lattice/docs/installation):

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

`--no-modify-path` skips editing a shell rc file, which a CI step never sources.
The binary lands at `.lattice/bin/lattice`; call it by that path, or add
`.lattice/bin` to the job's `PATH` yourself.

## The output mode a CI job gets

`lattice run` and `lattice setup` pick their output mode the same way
everywhere: stdout not attached to a terminal, the `CI` environment variable
set, or `--loquacious`/`-l` passed all give `Raw` — a plain, line-by-line stream
instead of the live interactive display. See [Output and
logging](/lattice/docs/output-modes) for the full model.

A CI job satisfies two of those triggers at once: GitHub Actions sets `CI` in
every job's environment, and a step's stdout isn't a terminal. `CI` and `-l` are
interchangeable as mode triggers; `-l` additionally turns on per-task output and
hash trace lines. Somewhere that doesn't set `CI` — a plain SSH session driving
a build — `-l` gets you the same stream:

```sh
lattice run build -l
```

Color follows the terminal rather than the mode, so a CI log is ANSI-free
either way.

## Exit codes and what fails a job

`lattice run` exits `0` when every task in the run finished successfully,
including runs with nothing to do: no workspaces declared, or a `--filter` that
matched nothing, print a message and still exit `0`. Anything else exits `1` — a
task's command exiting nonzero, an unresolvable driver, a task name that isn't
in `lattice.json`, a config that fails to load. That nonzero exit is what fails
the CI step.

## Collecting every failure in one run

By default a run stops as soon as one task fails, so a broken `lint` step keeps
`test` and `build` from starting. Pass `--continue` to keep running every task
whose dependencies didn't fail, and still exit `1` at the end if anything did:

```sh
lattice run lint test build --continue
```

The run's summary line names how many tasks failed and how many downstream tasks
were skipped because a dependency of theirs failed.

## Concurrency on a constrained runner

`--concurrency` caps how many tasks run at once. Without it, Lattice uses the
number of CPUs the machine reports. On a 2- or 4-vCPU runner, tasks that are
already internally parallel or memory-heavy end up competing for the same cores
under that default:

```sh
lattice run build --concurrency 4
```

## Persisting the cache between runs

Every job checks out a fresh copy of the repo, so the cache starts empty unless
you restore it. Restore and save one directory: `.lattice/cache` by default, or
wherever `settings.cacheDir` points:

```json
{
  "settings": {
    "cacheDir": ".lattice/cache"
  }
}
```

The cache action's `path` and `settings.cacheDir` must name the same directory.

What invalidates an entry inside that directory has nothing to do with the CI
cache action's own key — see [Caching](/lattice/docs/caching) for the full model.
Each entry is content-addressed by the task's command, inputs, environment,
lockfiles, toolchain, and the Lattice version; a lookup is a hit only if that key
matches and the stored artifact's digest checks out. The directory you restore
doesn't need to match the current commit: entries left over from an older commit
never get looked up again. They don't cause a wrong result, they're dead weight
until pruned. Restore on a broad, rolling key — by OS, not by lockfile hash.

`actions/cache`'s save step is skipped whenever the run gets an exact key hit on
restore, which would freeze a static key after its first save. Give the save a
key that's always new, and fall back to the newest previous save on restore:

```yaml
- name: Restore the Lattice cache
  uses: actions/cache/restore@v6
  with:
    path: .lattice/cache
    key: lattice-${{ runner.os }}-${{ github.run_id }}
    restore-keys: |
      lattice-${{ runner.os }}-

- name: Save the Lattice cache
  if: always()
  uses: actions/cache/save@v6
  with:
    path: .lattice/cache
    key: lattice-${{ runner.os }}-${{ github.run_id }}
```

`if: always()` on the save step means a run that fails partway still saves
whatever the cache picked up before the failure.

## Keeping the saved cache bounded

Every run re-uploads the whole cache directory to the CI cache action's storage,
so an unbounded cache costs upload time on every job. Set a budget and each run
holds itself to it:

```json
{
  "settings": {
    "maxCacheSize": "2GB"
  }
}
```

Eviction is least-recently-used, so what goes is whichever entries haven't been
hit in the longest stretch of runs. To use a different limit in CI than locally,
run `lattice prune` before the save step:

```sh
lattice prune --max-size 2GB
```

With neither the setting nor the flag, `prune` fails rather than guess at a
limit, and the cache grows without bound. See
[Caching](/lattice/docs/caching#keeping-the-cache-to-a-size) for the rest of the
model.

## Bounding how long a task can run

A task that hangs holds the job until the runner's own timeout kills it, which
costs the whole remaining budget and leaves no cache saved. Give the tasks that
can hang a `timeout`:

```json
{
  "tasks": {
    "test": { "timeout": "10m" },
    "build": { "timeout": "20m" }
  }
}
```

An overrun stops the task's whole process group and counts it as a failure, so
the run ends the way any other failure ends and the steps after it still get to
save the cache.

## Cancelled jobs

Cancelling a job sends `SIGTERM`. Lattice passes it on to every running task's
process group, gives them five seconds, then kills what's left, and exits `130`
rather than `1` — a cancellation is not a build that broke. Without this, tasks
spawned into their own process groups would outlive the runner's shutdown.

## `lattice setup` as its own step

`lattice run` doesn't install dependencies or provision toolchains. Run `lattice
setup` before `lattice run` in every job:

```sh
lattice setup
```

`setup` provisions anything declared under `engines` first, so dependency
installers see the pinned `PATH`, then installs each workspace's native
dependencies with whatever package manager it detected (or the workspace's
declared `scripts`, if `auto: false`). A fresh checkout has no local marker to
compare against, so `setup` installs every workspace's dependencies on the first
run in a job. Toolchains provisioned this way land under `.lattice/toolchains`, a
separate directory from the task cache — see [Engines and
provisioning](/lattice/docs/engines) if you want to persist that between runs
too.

## A complete workflow

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7

      - name: Install lattice
        run: |
          curl -fsSL https://latticeandcompany.github.io/lattice/install.sh \
            | sh -s -- --no-modify-path

      - name: Restore the Lattice cache
        uses: actions/cache/restore@v6
        with:
          path: .lattice/cache
          key: lattice-${{ runner.os }}-${{ github.run_id }}
          restore-keys: |
            lattice-${{ runner.os }}-

      - name: Set up toolchains and dependencies
        run: ./.lattice/bin/lattice setup

      - name: Run the pipeline
        run: |
          ./.lattice/bin/lattice run lint test build \
            --continue --concurrency 4

      - name: Keep the cache bounded
        if: always()
        run: ./.lattice/bin/lattice prune --max-size 2GB

      - name: Save the Lattice cache
        if: always()
        uses: actions/cache/save@v6
        with:
          path: .lattice/cache
          key: lattice-${{ runner.os }}-${{ github.run_id }}
```

`if: always()` on the last two steps means a failed `lint`, caught by
`--continue`, still gets its cache pruned and saved. The job still fails:
`lattice run`'s nonzero exit propagates out of the `run:` step regardless of what
runs after it.

This repo's own CI (`.github/workflows/ci.yml`) builds and tests the Lattice
binary directly with `cargo` rather than through `lattice run`, since there's no
built Lattice yet at that point in its own pipeline. The pins above
(`actions/checkout@v7`, `actions/cache/restore@v6`, `actions/cache/save@v6`) are
current majors whose `action.yml` declares `using: node24`. Check that before
pinning an action; a major still on the Node 20 runtime is deprecated.

## Next

- [CLI reference](/lattice/docs/cli) — every flag `run`, `setup`, and `prune`
  accept, and how flag, environment, and `settings` precedence works.
- [Output and logging](/lattice/docs/output-modes) — the full model behind `Raw`
  vs. interactive output.
- [Caching](/lattice/docs/caching) — what feeds a task's cache key and the
  integrity rule that makes a stale or partial cache directory safe to restore.
