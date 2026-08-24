---
title: Run Lattice in CI
description: Install it in a job, persist the cache, bound the run, and read the exit code.
group: Guides
order: 6
---

# Run Lattice in CI

Running Lattice on a build machine is the same `lattice setup` and `lattice run`
you use locally. Two things differ: the output a job produces, and the fact that
its working directory does not survive to the next run.

## Install it in a job

A job starts with no `lattice` binary, so install one first:

```sh
curl -fsSL https://latticeandcompany.github.io/lattice/install.sh | sh -s -- --no-modify-path
```

`--no-modify-path` skips editing a shell rc file, which a CI step never sources.
The binary lands at `.lattice/bin/lattice`. Call it by that path, or add
`.lattice/bin` to the job's `PATH` yourself. See
[Installation](/lattice/docs/installation).

On a Windows runner, the same script works in a `bash` step, and `install.ps1`
works in a PowerShell one:

```powershell
$env:LATTICE_NO_PATH = '1'
irm https://latticeandcompany.github.io/lattice/install.ps1 | iex
```

A CI shell is not interactive, so `install.ps1` leaves the user `PATH` alone
whether or not you set that variable. Setting it says so out loud, and stops the
job depending on how the runner reports interactivity. The binary is at
`.lattice\bin\lattice.exe`.

## Run `lattice setup` as its own step

`lattice run` does not install dependencies or provision toolchains. Put
`lattice setup` before it in every job:

```sh
lattice setup
```

`setup` provisions everything declared under `engines` first, so dependency
installers see the pinned `PATH`, then installs each workspace's dependencies
with whatever package manager it detected. A fresh checkout has no local marker
to compare against, so the first `setup` in a job installs every workspace's
dependencies. Toolchains land under `.lattice/toolchains`, a separate directory
from the task cache. See [Pinning tool
versions](/lattice/docs/pinning-tool-versions).

## What output a job gets

`lattice run` and `lattice setup` pick their output mode the same way
everywhere. Lattice prints raw, line-by-line output when stdout is not a
terminal, when the `CI` environment variable is set, or when you pass
`-v`/`--verbose`. A CI job satisfies the first two at once: a step's stdout
is not a terminal, and GitHub Actions sets `CI` in every job.

`CI` and `-v` are interchangeable as mode triggers. `-v` additionally turns on
per-task output and hash trace lines. Somewhere that does not set `CI`, such as
an SSH session driving a build, pass `-v`:

```sh
lattice run build -v
```

Color follows the terminal rather than the mode, so a CI log is free of escape
codes either way. See [Output and logging](/lattice/docs/output-modes).

Two lines in that log are worth grepping for. The summary is always the last one
a successful run prints:

```text
lattice: 10 tasks, 8 cached, 0 failed, 4.20s, 2m 51s saved
```

The saved figure is appended after the elapsed time rather than inserted before
it, and it is left off when it is zero, so a job that greps the counts reads the
same as it always has. It is task time, not wall clock: each hit contributes the
time the run that wrote its entry spent, so eight hits on a parallel graph can
report more saved time than the job took.

A run where every scheduled task came back from cache adds one more line under
the summary:

```text
lattice: full power, nothing to run
```

That marker needs at least one task scheduled, no failures, and a hit for every
task, so a `--filter` that matched no workspace never prints it. If a CI check
of yours greps for `lattice: full cache, nothing to run`, that is the string
this one replaced.

## Exit codes

`lattice run` exits `0` when every task finished successfully, including runs
with nothing to do. No workspaces declared, or a `--filter` that matched
nothing, prints a line and still exits `0`.

| Exit | Means |
| --- | --- |
| `0` | Every task succeeded, or there was nothing to run |
| `1` | A task's command failed, or the config or graph could not be resolved |
| `130` | The run was interrupted by `SIGINT` or `SIGTERM` |

The `130` is what lets a runner tell a cancelled job from a broken build.
Cancelling a job sends `SIGTERM`. Lattice passes it on to every running task's
process group, gives them five seconds, then kills what is left. Without that
pass-on, tasks spawned into their own process groups would outlive the runner's
shutdown.

`SIGTERM` ends the run wherever it arrives, including a run a `persistent` task
is holding open, such as a job that starts a dev server and tests against it.
Such a run used to ignore `SIGTERM` and hang until the runner force-killed it.
The hang cost a cancelled job the rest of its timeout and left the step's result
unreported. A cancel now stops the run on the signal itself.

Lattice does not report a task stopped that way as a failure. The log shows the
tasks that were running and then the summary, with no `FAILED` line for the ones
the cancel stopped. A `0 failed` summary next to exit `130` is correct, so read
the exit code rather than the summary to tell a cancel from a failure.

## Collect every failure in one run

By default one failure stops the run, so a broken `lint` keeps `test` and
`build` from starting. To keep running everything whose dependencies did not
fail, pass `--continue`:

```sh
lattice run lint test build --continue
```

The run still exits `1` if anything failed. The summary line carries the failed
count, and each task skipped because a prerequisite failed prints
`skipped (dependency failed)` under `-v`. Each failure prints one line naming
the exit code and how long the task ran, `api:test: FAILED (code 1) after
12.40s`, with the task's captured output under it. A search for `FAILED` in the
log finds both the tasks that broke and what they printed.

## Cap parallelism on a small runner

Without `--concurrency`, Lattice uses the number of CPUs the machine reports. On
a 2- or 4-vCPU runner, tasks that are already internally parallel or
memory-heavy compete for the same cores under that default:

```sh
lattice run build --concurrency 4
```

## Bound how long a task can run

A task that hangs holds the job until the runner's own timeout kills it, which
costs the whole remaining budget and saves no cache. Give the tasks that can
hang a `timeout`:

```json
{
  "tasks": {
    "test": { "timeout": "10m" },
    "build": { "timeout": "20m" }
  }
}
```

An overrun stops the task's whole process group and counts as a failure, so the
run ends the way any other failure ends and later steps still get to save the
cache. A task stopped that way has no exit code, so its line names only the
time, `api:test: FAILED after 10:00`. The captured output ends with `timed out
after 10m and was stopped`. `timeout` accepts `ms`, `s`, `m`, and `h`, or a bare
integer of seconds. A `persistent` task ignores it.

## Persist the cache between runs

Every job checks out a fresh copy of the repo, so the cache starts empty unless
you restore it. Restore and save one directory: `.lattice/cache` by default, or
wherever `settings.cacheDir` points.

```json
{
  "settings": {
    "cacheDir": ".lattice/cache"
  }
}
```

The cache action's `path` and `settings.cacheDir` must name the same directory.
`settings.cacheDir` has to name a directory inside the repo, and it cannot be
the repo root, so a path under the runner's home directory is not an option.
Point the cache action at the repo-relative path instead.

`actions/cache`'s save step is skipped whenever the restore got an exact key
hit, which would freeze a static key after its first save. Give the save a key
that is always new, and fall back to the newest previous save on restore:

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

Restore on a broad, rolling key: by OS, not by lockfile hash. What invalidates
an entry inside the directory has nothing to do with the cache action's key.
Each entry is content-addressed, and a lookup is a hit only if that key matches
and the stored artifact's digest checks out. So the directory you restore does
not need to match the current commit. Entries left over from an older commit are
never looked up again, and they cost storage rather than correctness. See
[Caching](/lattice/docs/caching).

The ledger `lattice stats` reads is a file in that same directory, so a restored
cache brings the run history with it and `stats` in a job reports across the jobs
that shared it. Jobs that restore the same snapshot in parallel each append to
their own copy and only one save wins, so a CI history is lossier than a single
machine's.

## Keep the saved cache bounded

Every run re-uploads the whole cache directory to the cache action's storage, so
an unbounded cache costs upload time on every job. Set a budget and each run
holds itself to it:

```json
{
  "settings": {
    "maxCacheSize": "2GB"
  }
}
```

Eviction is least-recently-used, so what goes is whatever has not been hit in
the longest stretch of runs. To use a different limit in CI than locally, run
`lattice prune` before the save step:

```sh
lattice prune --max-size 2GB
```

With neither the setting nor the flag, `prune` fails rather than guess a limit,
and the cache grows without bound. See
[Caching](/lattice/docs/caching#keeping-the-cache-to-a-size).

A `prune` here does not reclaim debris the run just left behind. A leftover has
to sit untouched for an hour first, so `prune` frees only the entries the size
limit evicts. The wait keeps two steps that share one checkout from deleting each
other's cache writes. If a step runs `lattice` while another `lattice` is still
going, neither loses what it stored.

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
`--continue`, still gets its cache pruned and saved. The job still fails.
`lattice run`'s non-zero exit propagates out of the `run:` step regardless of
what runs after it.

Check the runtime an action declares before you pin it. Every major above
(`actions/checkout@v7`, `actions/cache/restore@v6`, `actions/cache/save@v6`)
declares `using: node24` in its `action.yml`. A major still on the Node 20
runtime is deprecated.

## Next

- [CLI reference](/lattice/docs/cli) for every flag `run`, `setup`, and `prune`
  accept, and how flag, environment, and `settings` precedence works.
- [Output and logging](/lattice/docs/output-modes) for the rest of the mode
  model.
- [Caching](/lattice/docs/caching) for what feeds a cache key and the integrity
  rule that makes a stale cache directory safe to restore.
