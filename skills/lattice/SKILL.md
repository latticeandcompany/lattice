---
name: lattice
description: >
  Work with Lattice — the `lattice` CLI and its `lattice.json` — to run and cache
  tasks across a monorepo in any language and to pin tool versions. Use this skill when:
  (1) the repo contains a `lattice.json`,
  (2) the user asks to build, test, lint, or run anything through `lattice`,
  (3) adding Lattice to a repo or declaring a new workspace or task,
  (4) a run fails with a driver, cache, engine, or task-graph error,
  (5) wiring `lattice` into CI.
license: ISC
compatibility: lattice 1.0.0-beta-2+
metadata:
  author: latticeandcompany
  version: "1.0.0"
allowed-tools: Bash(lattice:*) Read Write Edit Glob Grep
---

# Lattice

Lattice is one CLI with two halves, usable together or apart. It runs a
monorepo's tasks in dependency order and in parallel, caching each result by
content so unchanged work is skipped. It also pins the versions of the tools
those tasks use. Everything it knows comes from one file: `lattice.json` at the
repo root.

Lattice does not replace the commands a project already has. Each workspace runs
its real command — `cargo test`, `pnpm run build`, `./gradlew check` — either
detected from the directory or written out in `scripts`.

## Orient before you act

1. Find the config. Every command walks up from the current directory to the
   nearest `lattice.json` and treats that directory as the repo root, so you can
   run `lattice` from a subdirectory. No config in this directory or any parent
   is a hard error.
2. Read `lattice.json` end to end. `workspaces` tells you what the repo is
   divided into, `tasks` tells you what verbs exist, `engines` tells you which
   tool versions are enforced.
3. Ask the binary, not the config, what a task will do:

   ```sh
   lattice run build --dry-run
   ```

   This loads and validates the config, resolves every `workspace:task` node and
   its exact shell command, prints them in dependency order, and exits without
   running or caching anything. It is the cheapest way to confirm both that your
   edit to `lattice.json` parses and that the commands are the ones you meant.

Run a task name that isn't a key under `tasks` and Lattice fails, listing the
names that do exist. `lattice run --help` and `lattice <command> --help` are
authoritative if anything here disagrees with the installed binary.

## Rules that will bite you

- **A persistent task blocks until interrupted.** Once a run starts a task
  declared `persistent: true`, `lattice run` streams its output and waits for
  Ctrl-C before exiting — by design, since a dev server has no completion to
  wait for. Never call one from a blocking foreground command. Run it in the
  background, or run only its non-persistent prerequisites.
- **`--filter` does not pull in dependencies.** It keeps workspaces whose `name`
  *contains* the pattern (substring, not glob, matched on `name` and never on
  `path`), and it is applied before the graph is built, so an edge into a
  filtered-out workspace is never created. Use it to re-run one workspace whose
  dependencies you know are current. For "and everything it needs," drop the
  filter — current dependencies come back from cache.
- **A task with no `inputs` has no files in its cache key.** It hits on every
  re-run with an unchanged command, including right after you edit the source it
  builds from. That is a stale result served confidently, not a bug. Declare
  `inputs`, or set `cache: false` until you're ready to.
- **A workspace `path` is a literal directory.** `packages/*` is treated as a
  directory named `*` and fails. One entry per project directory.
- **Unknown keys in `lattice.json` are silently ignored by the parser.** A
  typo'd field has no effect and raises nothing. The bundled
  `.lattice/schema.json` rejects it, so keep `"$schema": ".lattice/schema.json"`
  in the file and treat editor validation as the check for typos.
- **`lattice run` installs nothing.** It expects dependencies and toolchains to
  be in place. `lattice setup` is what provisions `engines` and runs each
  workspace's native installer.
- **`lattice` never invents a driver.** When a workspace's evidence is ambiguous
  or absent, the run halts with the fix printed. Don't work around a halt by
  guessing a command into `scripts` — see [When detection halts](#when-detection-halts).

## Running tasks

```sh
lattice run build                      # one task, across every workspace that has it
lattice run lint test build            # merged into one graph; shared deps run once
lattice run lint test build -s         # one graph per task, each to completion
lattice run test --filter api          # only workspaces whose name contains "api"
lattice run build --dry-run            # resolve and print, run nothing
lattice run lint test --continue       # keep going past a failure, still exit 1
lattice run build --concurrency 4      # cap parallelism (default: logical CPUs)
lattice run build --no-cache           # ignore the cache this run (--force is an alias)
lattice run build -l                   # plain line-by-line log instead of the live display
```

`-l`/`--loquacious` is worth passing whenever you need to read or grep the
output: it prints each task's hash and cache outcome as plain lines. Output is
already in that mode when stdout isn't a terminal or `CI` is set, so a command
you run programmatically usually gets it for free.

Exit codes: `0` success, including a run whose filter matched nothing and a repo
with an empty `workspaces` array; `1` any error Lattice raises, including a
failed task; `2` the command line itself was rejected.

## Editing `lattice.json`

Five top-level keys, all optional — `{}` is a valid config.

```json
{
  "$schema": ".lattice/schema.json",
  "latticeVersion": "1.0.0-beta-2",
  "workspaces": [
    { "name": "core", "path": "libs/core" },
    { "name": "api", "path": "services/api", "dependsOn": ["core"] },
    {
      "name": "docs",
      "path": "docs",
      "auto": false,
      "scripts": { "build": "python3 -m mkdocs build" }
    }
  ],
  "engines": { "node": ">=20.0.0" },
  "tasks": {
    "build": {
      "dependsOn": ["^build"],
      "inputs": ["src/**/*", "package.json"],
      "outputs": ["dist/**"]
    },
    "test": { "dependsOn": ["build"], "inputs": ["src/**/*", "tests/**/*"] },
    "dev": { "persistent": true }
  },
  "settings": { "maxCacheSize": "10GB" }
}
```

A workspace needs only `name` and `path`. `auto` defaults to `true`, which means
the driver and each task's command are read from the directory's own evidence.
`scripts` maps a task name to an exact shell command and always wins over
anything inferred, whether or not `auto` is true — so you can override one task
and leave the rest detected. With `auto: false` nothing is inferred and
`scripts` is the only source of commands; a task requested against such a
workspace with no matching entry fails rather than being skipped.

In a task's `dependsOn`, `^build` means "`build` in each workspace this one
depends on," following that workspace's own `dependsOn`; a bare `build` means
"`build` in this same workspace." Those two tokens are the whole vocabulary —
there is no glob and no `workspace#task` form. A workspace's `dependsOn` on its
own changes no ordering; it only takes effect where some task uses `^`.

Full field-by-field reference, including every type and default:
`references/configuration.md`.

## Caching decisions

The cache key is a hash over the task name and its resolved command, the
contents of every file matched by `inputs` minus `ignore`, any tool-unique
lockfile in the workspace, the resolved values of the variables named in `env`,
the resolved toolchain identity, and the Lattice version. Nothing else is
consulted.

That makes four fields yours to get right:

- `inputs` — what the command reads. Under-declare and you get a stale hit;
  over-declare and unrelated edits force rebuilds.
- `ignore` — subtracts from what `inputs` matched. Reach for it when a broad
  glob sweeps in logs, an inner tool's cache directory, or the task's own
  output.
- `outputs` — what gets archived on success and restored on a hit. A file the
  command produces that no `outputs` glob matches is never saved. `test` and
  `lint` usually need none.
- `env` — variable *names*, not `NAME=value` pairs. The resolved value is
  hashed and exported to the task. An unset name contributes nothing. Lattice
  does not read `.env` files.

Set `cache: false` to opt one task out entirely. `persistent: true` is never
cached regardless. The cache lives in `.lattice/cache` (move it with
`settings.cacheDir`) and is safe to delete at any time. It only grows until
`lattice prune` evicts least-recently-used entries down to `--max-size` or
`settings.maxCacheSize`; with neither set, `prune` fails rather than pick a
limit.

## Pinning tool versions

An `engines` entry's *shape* — nothing else, not the tool's name — selects one
of three behaviors:

| You write | What happens |
| --- | --- |
| Nothing, or `{}` | Uses whatever is on `PATH`. Checks nothing. |
| A version constraint, no `installCmd` | Runs the tool's version command on the host and fails if it doesn't satisfy the constraint. Installs nothing. |
| An `installCmd` | Installs into `.lattice/toolchains/<engine>/<version>-<hash>/`, version-checks it, pins it, and prepends its `bin` to that task's `PATH`. |

The bare string form (`"node": ">=20.0.0"`) works only for engines Lattice has
a built-in version rule for. Any other name needs the object form with an
explicit `versionCmd`, or the config is rejected on load. Root `engines` are
defaults; a workspace's own `engines` win per key. The well-known list, the
built-in driver table, and the `installCmd` contract are in
`references/toolchains.md`.

Nothing about a provisioned engine touches the shell, a profile, or a global
install — the `PATH` change lives in the one child process Lattice spawns for
that task, and `rm -rf .lattice/toolchains` is a complete uninstall.

## When detection halts

```text
Error: workspace 'app' has an ambiguous or undeclared task driver.
Candidate tools seen: pnpm, npm, yarn, bun
Declare the task driver explicitly by adding to this workspace in lattice.json:
  "engines": { "pnpm": ">=0.0.0" }
```

Lattice climbs a fixed ladder per workspace and stops at the first rung that
names exactly one tool: an `engines` declaration, then a file the developer
authored to pin a tool (`packageManager`, `.tool-versions`, `rust-toolchain.toml`,
`./gradlew`), then a lockfile only one tool produces. A bare `package.json`,
`pom.xml`, or `pyproject.toml` identifies an ecosystem, not a tool, and is never
enough on its own. Two tools of the same role — two package managers, two build
tools — conflict; different roles compose, so a runtime plus a package manager
is fine and a task runner sitting above a package manager drives.

Two correct fixes, and the choice is which matches reality:

- Add the suggested `engines` entry naming the tool actually used. A
  declaration is rung 1, so it settles the question permanently. Replace the
  `>=0.0.0` placeholder with a real constraint.
- Set `"auto": false` and write `scripts`. Correct when there is no single
  right answer to infer — the real build step is a wrapper script, or the
  workspace is itself a repo with its own task runner underneath.

Ask the user which tool the workspace actually uses if the evidence doesn't say.
Don't pick one from the candidate list to make the error go away.

## Adding Lattice to a repo

Order matters; skipping ahead is what makes an adoption feel like a rewrite.

1. `lattice init` (add `-y` to skip the wizard and write the skeleton). It
   writes `lattice.json`, a committed `.lattice/schema.json`, and three
   `.gitignore` lines. Without a TTY it always writes the skeleton and never
   prompts.
2. Declare one workspace and one bare task (`"build": {}`). Confirm with
   `lattice run build --dry-run` that the resolved command is the one that
   directory already runs by hand.
3. Only then add `inputs` and `outputs`.
4. Pin `engines` last, and only where the guarantee is actually needed.

Nothing about declaring a workspace stops its existing `package.json` scripts,
`Makefile`, or CI step from working. Bring workspaces in one at a time and use
`--filter` to scope a run while you migrate.

A subtree that already has its own task runner does not need flattening:
declare it as one workspace with `auto: false` whose `scripts` call that runner,
give it a broad `inputs` glob with the inner cache directory and `node_modules`
in `ignore`, and Lattice schedules and caches it as a single node. Flatten it
instead when another workspace needs to depend on one specific inner package, or
when you want a rebuild narrowed below the whole subtree.

## In CI

`lattice setup` as its own step, then `lattice run`. Restore and save the cache
directory (`.lattice/cache`, or `settings.cacheDir`) with a rolling key, and run
`lattice prune --max-size <size>` before saving so it stays bounded. `--continue`
is the right shape for a CI run: it reports every failure in one pass and still
exits `1`. `CI` being set already forces plain output, so `-l` is redundant
there.

## Verify your work

- `lattice run <tasks> --dry-run` — the config parses and the commands are right.
- `lattice run <tasks> -l` — real run, with each task's hash and cache outcome
  on its own line.
- Run it a second time. Every task should report a cache hit. If a task misses
  twice in a row with nothing changed, something in its `inputs` or `env`
  changes every run.
- `lattice version` — which binary actually ran, which matters when the repo
  pins `latticeVersion`.

## References

- `references/configuration.md` — every `lattice.json` field, type, default, and
  validation error.
- `references/cli.md` — every command, flag, exit code, environment variable,
  and the precedence between them.
- `references/toolchains.md` — the built-in driver table, the well-known engine
  list, and provisioning mechanics.
- `references/troubleshooting.md` — symptom to cause to fix.

Full documentation: <https://latticeandcompany.github.io/lattice/docs>
