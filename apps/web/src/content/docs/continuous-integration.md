---
title: Continuous integration
description: Running lattice on a build machine, caching between runs, and reading exit codes.
group: Guides
order: 6
---

# Continuous integration

Running Lattice in CI is the same `lattice setup` and `lattice run` you use on
your own machine. The parts worth planning for are the ones a build machine
does differently from a laptop: the output it produces, what makes a job fail,
and how the cache survives between one run and the next when the working
directory itself doesn't.

## Installing lattice in a job

A job starts with no `lattice` binary, so the first step installs one. The
installer is the same one-liner from [Installation](/lattice/docs/installation):

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

`--no-modify-path` skips editing a shell rc file — a CI step doesn't source
one, so there's nothing to gain by writing it, and the flag avoids a wasted
edit. The binary lands at `.lattice/bin/lattice`; call it by that path (or add
`.lattice/bin` to the job's `PATH` yourself) rather than relying on a shell
config that never gets read.

## The output mode a CI job gets

`lattice run` and `lattice setup` pick their output mode the same way
everywhere: not attached to a terminal, or the `CI` environment variable is
set, or `--loquacious`/`-l` was passed, and the mode is `Raw` — a plain,
line-by-line stream instead of the live interactive display. See
[Output and logging](/lattice/docs/output-modes) for the full model.

A CI job gets `Raw` for two independent reasons at once: GitHub Actions sets
`CI` in every job's environment, and a step's stdout isn't a terminal to begin
with. Either one alone is already enough. That second reason is also why a CI
log is ANSI-free: color follows the terminal, not the mode, so the colored
`workspace:task` labels an `-l` run shows at a shell never reach a log file. You never need `-l` in a GitHub
Actions job for this reason — but if you're running Lattice somewhere that
doesn't set `CI` (a plain SSH session driving a build, for instance) and want
the same greppable output, pass `-l` explicitly:

```sh
lattice run build -l
```

## Exit codes and what fails a job

`lattice run` exits `0` when every task in the run finished successfully,
including runs with nothing to do — no workspaces declared, or a `--filter`
that matched nothing print a message and still exit `0`; an empty repo is not
a failure. Anything else — a task's command exiting nonzero, an unresolvable
driver, a task name that isn't in `lattice.json`, a config that fails to
load — exits `1`. That nonzero exit is what fails the CI step; there's nothing
else to configure.

## Collecting every failure in one run

By default a run stops as soon as one task fails, so a broken `lint` step
keeps `test` and `build` from ever starting. Pass `--continue` to keep running
every task whose dependencies didn't fail, and still exit `1` at the end if
anything did:

```sh
lattice run lint test build --continue
```

This is the shape worth using in CI: it reports every failure the run turned
up in one pass instead of one failure per push. The run's summary line names
how many tasks failed and how many downstream tasks were skipped because a
dependency of theirs failed.

## Concurrency on a constrained runner

`--concurrency` caps how many tasks run at once; without it, Lattice uses the
number of CPUs on the machine. A shared or small runner (a 2- or 4-vCPU GitHub
Actions runner, for instance) can do worse under the unconstrained default than
under a lower one, if the tasks themselves are already parallel or memory-heavy
processes competing for the same cores:

```sh
lattice run build --concurrency 4
```

There's no single right number — it depends on how heavy your tasks are and
how much the runner actually has. Start from the runner's advertised CPU count
and adjust from there.

## Persisting the cache between runs

Every job checks out a fresh copy of the repo, so without help the cache
starts empty on every run. Restore and save one directory: `.lattice/cache` by
default, or wherever `settings.cacheDir` points if you've moved it:

```json
{
  "settings": {
    "cacheDir": ".lattice/cache"
  }
}
```

Setting `cacheDir` explicitly is mainly useful so the path you tell a cache
action to restore and save doesn't drift out of sync with the config if
someone later changes it — the action's `path` and `settings.cacheDir` must
name the same directory.

What actually invalidates an entry inside that directory has nothing to do
with the CI cache action's own key — see [Caching](/lattice/docs/caching) for
the full model. Each entry is content-addressed by the task's command,
inputs, environment, lockfiles, toolchain, and the Lattice version; a lookup
is a hit only if that exact key matches and the stored artifact's digest
checks out. That means the directory you restore doesn't need to match the
current commit at all — entries left over from an older commit that no
longer apply simply never get looked up again; they don't cause a wrong
result, they're just dead weight until pruned. A broad, rolling restore key
(by OS, not by lockfile hash) is the right shape here, which is different from
how you'd cache a package manager's own store.

`actions/cache`'s save step is skipped whenever the run gets an exact key hit
on restore, which would freeze a static key after its first save and stop the
cache from ever picking up new entries. Give the save a key that's always new,
and fall back to the newest previous save on restore:

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

`if: always()` on the save step means a run that fails partway (with
`--continue` or without it) still saves whatever the cache picked up before
the failure.

## Keeping the saved cache bounded

The directory you save only grows — nothing evicts an entry on its own — and
every run re-uploads the whole thing to the CI cache action's own storage.
Run `lattice prune` before the save step to evict the oldest-used entries down
to a size limit:

```sh
lattice prune --max-size 2GB
```

Without `--max-size`, `prune` falls back to `settings.maxCacheSize`:

```json
{
  "settings": {
    "maxCacheSize": "2GB"
  }
}
```

and fails rather than guess at a limit if neither is set. Eviction is
least-recently-used, so pruning in CI removes whichever entries haven't been
hit in the longest stretch of runs first. See
[Caching](/lattice/docs/caching) for the rest of the pruning model.

## `lattice setup` as its own step

`lattice run` doesn't install dependencies or provision toolchains — it
expects that already done. Run `lattice setup` as a step before `lattice run`
in every job:

```sh
lattice setup
```

`setup` provisions anything declared under `engines` first, then installs
each workspace's native dependencies with whatever package manager it
detected (or the workspace's declared `scripts`, if `auto: false`). A fresh
checkout has no local marker to compare against, so `setup` installs every
workspace's dependencies on the first run in a job; that's expected; it's the
same thing a fresh clone does on a laptop. Toolchains provisioned this way
land under `.lattice/toolchains`, a separate directory from the task cache
this page covers — see [Engines and provisioning](/lattice/docs/engines) if
you also want to persist that between runs.

## A complete workflow

Putting the pieces together, for a repo that drives its jobs through
`lattice run` end to end:

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
`--continue`, still gets its cache pruned and saved rather than skipped
because an earlier step failed. The job itself still fails: `lattice run`'s
nonzero exit from the failed task propagates out of the `run:` step regardless
of what runs after it.

This repo's own CI (`.github/workflows/ci.yml`) builds and tests the Lattice
binary directly with `cargo`, rather than through `lattice run` — there's no
built version of Lattice yet at that point in its own pipeline to run itself
with. The action pins above (`actions/checkout@v7`, `actions/cache@v6`) follow
the same convention that repo's workflows use: a current major version, never
one whose `action.yml` still declares a deprecated Node runtime.

## Next

- [CLI reference](/lattice/docs/cli) — every flag `run`, `setup`, and `prune`
  accept, and how flag, environment, and `settings` precedence works.
- [Output and logging](/lattice/docs/output-modes) — the full model behind
  `Raw` vs. interactive output.
- [Caching](/lattice/docs/caching) — what feeds a task's cache key and the
  integrity rule that makes a stale or partial cache directory safe to
  restore.
