# Troubleshooting

Symptom first. Start every investigation with `lattice run <task> --dry-run`
(config validity plus resolved commands) and `lattice run <task> -l` (hash and
cache outcome per task).

## `no lattice.json found in this directory or any parent`

There is no config at or above the current directory. Either you're outside the
repo, or the repo doesn't use Lattice yet. `lattice init` creates one.

## `workspace 'x' has an ambiguous or undeclared task driver`

No rung of the evidence ladder named exactly one tool, two tools of the same role
are present at once, or the only candidates are runtimes, which cannot drive
tasks. Add the `engines` entry the error suggests, naming the tool actually used,
or set `"auto": false` and declare `scripts` — which is what the error suggests
when no candidate could drive. Ask which tool the workspace uses rather than
picking from the candidate list.

## A task's resolved command isn't what you expected

`scripts` always wins; only a task with no entry there falls back to the
driver's inferred invocation. Run `lattice run <task> --dry-run` — each line is
`workspace:task` next to the exact command that would be handed to the shell. If
it's wrong, add or edit that workspace's `scripts` entry.

`--dry-run` returns before toolchains are provisioned or `PATH` is adjusted, so
it shows the command as written, not necessarily as it would resolve if a
provisioned tool changes what's first on `PATH`.

## `workspace 'x' is "auto": false but declares no command for task 'y'`

A manual workspace must declare a script for every task it participates in. Add
the entry, or leave the workspace on `auto` so it can sit out tasks that don't
apply. Silent skipping is only for `auto: true` workspaces. Under `--filter`, the
check covers the workspaces the pattern matched; one pulled in as a dependency is
only asked for the tasks its dependents need.

## `cycle detected in task dependency graph`

Two tasks depend on each other, directly or through a chain, via same-workspace
or `^`-prefixed edges. The message names the graph, not the edge, so read each
involved task's `dependsOn` in `lattice.json`. Nothing ran.

## `persistent task 'dev' in workspace 'app' cannot be depended on by other tasks`

A persistent task must be a graph leaf — nothing can start after a process that
never finishes. If a task needs the artifact a dev server would serve, depend on
the build step that produces it.

## A config validation error on `lattice.json`

Duplicate workspace names, an empty `path`, a bad `maxCacheSize`, or a
string-form `engines` entry for a tool Lattice can't version-check are all caught
before any task runs, with the offending field and value named.

## `unknown field \`output\` in tasks.build (lattice.json line 5, column 14)`

A key that is not part of the config at that level. The message names the object
holding it (`workspaces[1]`, `tasks.build`, `engines.node`, or `at the top level
of lattice.json`), the position, the nearest valid field when there is one, and
every field accepted there. Nothing has run. Delete the key or correct it.

`output` for `outputs` and `input` for `inputs` are the reason this is fatal:
either one changes what the task caches. There is no way to keep an extra key in
the file, so a config carrying one from an older release has to drop it.

## A task never hits the cache

Something feeding the hash changes every run. Usually one of:

- an `inputs` glob matching a file that legitimately changes every run (a
  timestamp, `.DS_Store`, generated output) — narrow `inputs` or add it to
  `ignore`
- an `env` entry whose value differs across shells or machines (an absolute
  path, a session id) — drop it unless the command's result genuinely depends
  on it

`-l` shows the truncated hash and the hit/miss outcome per task, but not which
field moved the hash. Narrow it by elimination.

## A task hits the cache when it shouldn't

The inverse: the command reads a file or a variable that isn't declared. `inputs`
only matches what its globs say, and only names listed in `env` are hashed.
Neither case warns. A wider `inputs` glob or a fuller `env` list is the only fix.
Use `--no-cache` (or `--force`) to force a fresh run while investigating.

## The output looks wrong after a cache hit

A lookup is a hit only if the metadata parses, the tarball opens, and its sha256
matches what was recorded. Anything else is a miss, never a bad hit — a corrupt
or partial entry can only cost a re-run. If restoring outputs into the workspace
fails partway (permissions, disk space), that's a non-fatal warning and the task
did run, so the tree is correct even though a warning printed.

## An engine version check fails on one machine and not another

That constraint is in validate-only mode: a version range with no `installCmd`,
so Lattice checks whatever is on that machine's `PATH`. Either bring every
machine's host tool to the same version, or add an `installCmd` so Lattice
installs its own copy under `.lattice/toolchains/` and no host `PATH` matters.

## `installCmd` provisioning fails partway

Provisioning stages into a temporary directory, runs `installCmd`,
version-checks, then renames into the final content-addressed path. A failure at
any stage is fatal and names the stage. Nothing is left pinned, so rerunning
retries from scratch.

## `--dry-run` doesn't show toolchain problems

It returns before any engine is provisioned or validated. An engine failure only
surfaces on a real run, or up front via `lattice setup`.

## The interactive display doesn't appear

Output falls back to plain lines whenever stdout isn't a terminal, `CI` is set,
or `-l` was passed. On top of that, `lattice run` forces plain output whenever
the closure being run pulls in a `persistent` task. Redirecting into a file or a
pipe is the most common surprise cause.

## Color appears somewhere it shouldn't

Color is emitted only when stdout is a real terminal, in either mode: an `-l` run
at a shell colors each `workspace:task` label, the same run piped or redirected
colors nothing. Set `NO_COLOR` to any value to suppress it everywhere without
changing the output mode.

## A run never finishes

A `persistent: true` task in the closure is expected to block: once every other
task is done and the persistent one is spawned, `lattice run` streams its output
and waits for Ctrl-C. To get a run that terminates, don't request the persistent
task — run its non-persistent prerequisites instead.

A run only blocks while a persistent task is actually up. Lattice waits on every
persistent child, so one that exits is reported and stops holding the run open:

```text
web:dev: EXITED (code 1) after 1.09s
lattice: 2 tasks, 0 cached, 1 failed, 1.83s
```

Anything but a clean `0` counts as a failed task and exits non-zero; `exited
(code 0)` is reported on stdout and counts as nothing. When the last persistent
task exits, the run prints its summary without needing a Ctrl-C. A child Lattice
kills at shutdown is not reported and never counts as a failure.

## `--filter` ran more or fewer workspaces than expected

It's a substring match on workspace `name`, never on `path` and never a glob. The
matches are the roots of the run, so their transitive dependencies are in the
graph too, tagged `(dependency)` under `--dry-run`. Nothing that depends on a
match is included. A filter matching nothing exits `0`.

If a prerequisite you expected is missing, check that the depending workspace
lists it in `dependsOn` and that the task's own `dependsOn` carries the `^`.

## `.lattice/schema.json` is missing or your editor shows a stale schema

`run`, `setup`, and `prune` each self-heal a *missing* schema file. An existing
one is left alone on purpose. Delete it and rerun any command that loads config,
or force a rewrite with `lattice init --force`. It's the one thing under
`.lattice/` meant to be committed.

## Clean slate

Everything under `.lattice/` is derived state. `.lattice/cache/` costs only time
to rebuild; `.lattice/toolchains/` reprovisions on next need; `.lattice/bin/`
re-downloads the pinned release. `rm -rf .lattice` is a complete, safe reset and
never touches `lattice.json` at the repo root. You don't need to run
`lattice init` again afterward.

Before reporting a problem, gather `lattice version`,
`lattice run <tasks> --dry-run`, and `lattice run <tasks> -l`.
