---
title: Troubleshooting
description: Symptom, cause, and fix for the failure modes you actually hit.
group: Guides
order: 7
---

# Troubleshooting

This page starts from a symptom, not a message: something looks wrong and you
need to know why. For the exact text of every error Lattice can print, see
[Errors](/lattice/docs/errors) — this page links to it wherever a fix needs
you to have seen the specific message.

## Detection and configuration

### A workspace halts with "ambiguous or undeclared task driver"

Lattice climbs an evidence ladder to pick the tool that runs a workspace's
tasks — your `engines` declaration, then a native file like
`packageManager` or `.nvmrc`, then a tool-unique lockfile. If none of that is
present, or two tools of the same role are present at once (say,
`bun.lockb` and `pnpm-lock.yaml`), it halts rather than guess. See [Driver
detection](/lattice/docs/drivers) for the model and [Errors](/lattice/docs/errors#ambiguous-or-undeclared-task-driver)
for the exact message shapes.

Fix: paste the `engines` line the error suggests into that workspace's entry
in `lattice.json`, naming whichever tool you actually use. If you'd rather
not have anything inferred for that workspace, set `"auto": false` and
declare `scripts` yourself — the halt only ever fires for `auto: true`
workspaces.

### A task's resolved command isn't what you expected

Command resolution has a fixed precedence: an explicit entry in a
workspace's `scripts` map always wins; only a task with **no** entry there
falls back to the driver's inferred invocation
(`crates/lattice-workspace/src/lib.rs:846-851`). For JavaScript-family
drivers, inference requires the task name to exist in the manifest's own
`scripts`/`tasks` map — Lattice never invents a script your `package.json`
doesn't have. For direct-invoke drivers (`cargo`, `go`, and similar),
inference never fabricates a command for a `persistent` task — there's no
universal `cargo dev` — so a persistent task on one of those drivers needs
an explicit `scripts` entry.

Fix: run `lattice run <task> --dry-run` first. It prints a `dry run · build`
banner, then every `workspace:task` that would run, in order, next to its
fully resolved command — including cross-workspace dependencies — without
executing anything:

```sh
lattice run build --dry-run
```

```text
lattice  dry run · build
  → web:build   pnpm run build
  → cli:build   cargo build --release
```

If the command shown isn't the one you meant, add or edit that workspace's
`scripts` entry for the task; it takes precedence over anything inferred.

`--dry-run` returns before toolchains are provisioned or `PATH` is adjusted,
so it shows the command as written, not necessarily as it would resolve if a
provisioned tool changes what's first on `PATH`.

### A config validation error on `lattice.json`

Duplicate workspace names, an empty `path`, a bad `maxCacheSize` string, an
`engines` entry using the version-only string form for a tool Lattice
doesn't know how to version-check — all of these are caught before any task
runs, with a message naming the exact field and value. See
[Errors](/lattice/docs/errors#config-loading-and-validation) for every shape
and how to fix each. For the field reference itself, see
[Configuration](/lattice/docs/configuration).

### A cycle in the task graph

```text
cycle detected in task dependency graph
```

Two or more tasks depend on each other, directly or transitively, through
`dependsOn` (same-workspace or `^`-prefixed cross-workspace edges). There's
no valid run order, so nothing runs. Fix by tracing the `dependsOn` chain
back to where it loops — `lattice run <task> --dry-run` will not print an
order for a graph that can't be scheduled, but the error names the graph,
not a specific edge, so the fastest way to find the loop is to read
each involved task's `dependsOn` in `lattice.json` directly. See [Task
graph](/lattice/docs/task-graph) for how `^task` versus `task` builds edges.

## Caching

### A task never hits the cache

The cache key is a hash over the task name, its resolved command, every
file matched by that task's `inputs` globs, tool-unique lockfiles in the
workspace, the current values of any env vars listed in the task's `env`,
the resolved toolchain identity, and the Lattice version
(`crates/lattice-cache/src/lib.rs:336-339`). If the key changes on every run
even though nothing you'd call "the code" changed, the usual cause is
something feeding the hash that shouldn't:

- an `inputs` glob that matches a file which legitimately changes every run
  (a timestamp, a generated lockfile-adjacent file, `.DS_Store`) — narrow
  `inputs` or add it to `ignore`
- an `env` entry whose value differs across your shells or machines (an
  absolute path, a session id) — drop it from `env` unless the task's
  command output genuinely depends on it

Run with `-l`/`--loquacious` to see the truncated key and hit/miss status
per task as it runs:

```text
web:build: hash a1b2c3d4e5f6a7b8
web:build: cache miss
```

A different hash prefix on back-to-back runs with no source change points
at one of the inputs above. Loquacious mode shows the *outcome*, not which
field changed the hash — narrow it by elimination, or compare `inputs`/`env`
against what the build script actually reads.

### A task hits the cache when it shouldn't

The inverse problem: the build reads a file, or an env var, that isn't
declared. `inputs` only matches what its globs say — a file the build
reads outside of that set is invisible to `compute_key`, so editing it
changes nothing about the key, and the stale result gets replayed. Same
for `env`: only the names you list are read and hashed
(`crates/lattice-runner/src/lib.rs:509-516`); a var your command depends on
but that isn't listed can change value with no effect on the key. Neither
case raises a warning — a wider `inputs` glob or a fuller `env` list is the
only fix. See [Caching](/lattice/docs/caching) for what belongs in each,
and force a fresh run while you investigate with `lattice run <task>
--force` or `--no-cache`.

### `--force`/`--no-cache` doesn't help, or the output looks wrong after a hit

A cache hit only counts if the metadata parses, the tarball opens, and its
digest matches what's recorded — anything else is treated as a miss and the
task reruns, never a false hit
(`crates/lattice-cache/src/lib.rs`). If restoring the outputs into the
workspace itself fails partway (permissions, disk space), that's a
non-fatal warning and the task still ran fresh, so the code is correct even
though the warning printed. See
[Errors](/lattice/docs/errors#cache-operations) for the exact warning text.

## Toolchains

### An engine version check fails on a teammate's machine but not yours

This means the constraint is in **validate-only** mode — a version range
with no `installCmd` — so Lattice trusts whatever's on that machine's
`PATH` and only checks its version. Different machines with different
tools installed will disagree. See
[Errors](/lattice/docs/errors#validate-only-failures) for the exact
messages (command not found, version doesn't parse, doesn't satisfy the
range).

Fix: either get every machine's host tool onto the same version, or move
the constraint from validate-only to **provision** by adding an
`installCmd` — Lattice then installs its own copy into
`.lattice/toolchains/` per machine, so nobody's host `PATH` matters. See
[Engines and provisioning](/lattice/docs/engines) for the three-mode
gradient.

### `installCmd` provisioning fails partway

Provisioning stages the install into a temporary directory, runs
`installCmd`, version-checks the result, then renames the staged directory
into its final content-addressed path
(`crates/lattice-workspace/src/toolchain.rs`). A failure at any of those
steps — the install command itself exiting non-zero, the freshly installed
tool failing its version check, or the final rename failing — is fatal and
names the exact stage; see
[Errors](/lattice/docs/errors#provisioning-failures) for each message.
Nothing is left pinned on partial failure, so rerunning the same command
retries provisioning from scratch; there's no partial state to clean up by
hand.

### Toolchain resolution isn't shown by `--dry-run`

`--dry-run` prints resolved commands only — it returns before any
toolchain is provisioned or validated
(`crates/lattice/src/commands/run.rs`). An engine failure only surfaces
when a task actually runs (or via `lattice setup`, which provisions root
engines up front). If you need to confirm a teammate's toolchain resolves
before handing them a task to run, have them run `lattice setup` first.

## Output

### The interactive TUI doesn't show up

`detect_mode` picks `Raw` output whenever stdout isn't a real terminal, a
`CI` environment variable is set, or `-l`/`--loquacious` was passed —
otherwise it's `Interactive`
(`crates/lattice-output/src/lib.rs:22-35`). On top of that, `lattice run`
forces `Raw` whenever the tasks being run pull a `persistent` task into
their closure, regardless of the three checks above — a dev server's
streaming output is the point of the run, and a live TUI can't render it
underneath. Redirecting output to a file or a pipe also disables the TTY
check, which is the most common surprise cause. See [Output and
logging](/lattice/docs/output-modes) for the full model.

### You wanted plain lines but got the interactive view

Pass `-l`/`--loquacious`, set `settings.loquacious: true` in
`lattice.json`, or set `CI` in the environment — any one forces `Raw`. This
is also the fastest way to get a readable log when piping `lattice run`
output into another tool or a CI log viewer.

### Colored output shows up somewhere it shouldn't

Color is emitted only when stdout is a real terminal — that holds in both
modes, so an `-l` run at your shell has colored `workspace:task` labels while
the same run piped or redirected has none. Set `NO_COLOR=1` (any value) to
suppress it everywhere without changing the output mode itself. If you are
seeing escapes in something that isn't a terminal, whatever is running
Lattice is presenting itself as one.

## Running

### A persistent task never lets `lattice run` finish

This is expected, not a bug. Once a run starts a `persistent: true` task
(a dev server, a watcher), that task is detached and the run waits on a
shutdown signal before it will exit — non-persistent prerequisites still
run to completion and their results are visible first, but the process as
a whole blocks until you send `Ctrl-C`
(`crates/lattice-runner/src/lib.rs:441-461`). There's no flag that makes a
persistent task's own exit end the run, because the task is defined by
never exiting on its own. See [Persistent tasks](/lattice/docs/persistent-tasks)
and [Dev servers and watchers](/lattice/docs/dev-servers) for the intended
day-to-day shape of this — typically running the persistent task on its
own in one terminal, with everything else run separately.

If you want the run to fail fast instead of waiting on a task that will
never finish, don't include the persistent task in the requested task list
— run its non-persistent prerequisites on their own.

### A `--filter` runs fewer workspaces than you expected

`--filter <pattern>` keeps only workspaces whose **name** contains
`pattern` — a substring match, not a glob, and not a path match
(`crates/lattice/src/commands/run.rs`). It's applied before the graph is
built, so a filtered-out workspace has no node in the graph at all: if
another workspace depends on it, that edge is simply never created — it is
not run first, and it is not an error either. A filter matching nothing
prints `lattice: no workspaces matched filter '<pattern>'.` and exits 0,
not an error.

Fix: check the filter is matching on workspace `name` as declared in
`lattice.json`, not a directory name or a glob pattern. If you need a
dependency's task to run too, include enough of its name to match, or drop
the filter and use `--continue`/task selection instead. See [Selecting
what runs](/lattice/docs/filtering) for the full model, including why
dependencies are never pulled in implicitly.

### `.lattice/schema.json` is missing or your editor shows a stale schema

`lattice run`, `lattice setup`, and `lattice prune` each self-heal a
*missing* `.lattice/schema.json` by writing the bundled schema — but only
when the file is absent. An existing file, even an old one, is left alone
on purpose, to avoid churning a file you may have customized or committed
(`crates/lattice/src/schema.rs`).

Fix: if the file is present but your editor's JSON Schema validation looks
out of date, delete it and rerun any command that loads the config (`lattice
run ... --dry-run` is enough to trigger the self-heal), or force a full
rewrite with:

```sh
lattice init --force
```

`schema.json` is meant to be committed — it's the one thing under
`.lattice/` that `lattice init` does not add to `.gitignore`.

## Clean slate and gathering information

Everything under `.lattice/` is derived state:

- `.lattice/cache/` — cached task outputs, keyed by content. Deleting it
  costs nothing but time: every task re-runs cold on the next `lattice run`.
- `.lattice/toolchains/` — provisioned engines. Deleting it means any
  engine with an `installCmd` reprovisions from scratch next time it's
  needed — a network fetch, not a config change.
- `.lattice/bin/` — Lattice's own self-managed versions, if this repo pins
  `latticeVersion` to something other than whatever binary is on your
  `PATH`. Deleting it means the next command re-downloads and re-verifies
  the pinned release.
- `.lattice/schema.json` — the only one of these meant to be committed. If
  it's gone from disk but still tracked in git, a `git status` will show
  it as deleted; if it's genuinely gone, the next `run`/`setup`/`prune`
  rewrites it (see above).

`rm -rf .lattice` is a complete, safe reset: nothing under it is required
for correctness, only for speed and for not re-provisioning tools you
already have. It never touches `lattice.json` itself, which lives at the
repo root, not inside `.lattice/`. You do not need to run `lattice init`
again afterward — the schema self-heals and the cache and toolchains
rebuild as needed.

Before filing an issue or asking a teammate for help, gather:

- `lattice version` — confirms the exact binary version, useful when a repo
  pins `latticeVersion` and you're not sure which one actually ran.
- `lattice run <tasks> --dry-run` — confirms the graph and every resolved
  command without running or caching anything.
- `lattice run <tasks> -l` — reruns with raw, unbuffered output and cache
  hash/hit-miss trace lines, useful for both caching questions and for
  capturing full output to paste into a report.

For the underlying models these symptoms come from, see [Driver
detection](/lattice/docs/drivers), [Engines and
provisioning](/lattice/docs/engines), [Caching](/lattice/docs/caching),
[Task graph](/lattice/docs/task-graph), and [Output and
logging](/lattice/docs/output-modes). For the full message-by-message
reference, see [Errors](/lattice/docs/errors).
