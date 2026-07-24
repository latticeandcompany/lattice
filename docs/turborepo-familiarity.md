# Turborepo familiarity

Lattice's CLI is intentionally **familiar to Turborepo users — not a clone**.
Per the PRD (§1.1, *Turborepo Synergy*): Lattice is not designed to replace
Turborepo, but to coexist with it. You can wire an existing Turborepo-managed
sub-monorepo in as a single manual workspace inside a broader Lattice graph. The
CLI deliberately reuses the mental model and vocabulary Turbo users already know
— `run`, `--filter`, a `pipeline` — so there is nothing new to relearn. But
Lattice keeps its own branding, voice, help text, and output; this is shared
vocabulary, not copied wording.

## Command & flag mapping

| Turborepo | Lattice equivalent | Notes |
| :-- | :-- | :-- |
| `turbo run <task>` | `lattice run <task>` | Runs the task across your workspaces, resolving the DAG from the `pipeline`. |
| `turbo <task>` (bare shorthand) | `lattice run <task>` | The bare-task shorthand is **deferred** in Lattice — see below. Always use the explicit `run`. |
| `--filter <pattern>` | `--filter <pattern>` / `-f` | Scope the run to workspaces whose **name contains** `pattern` (substring match). |
| `--concurrency <N>` | `--concurrency <N>` | Cap how many tasks run at once. Default: number of CPUs (`available_parallelism`). |
| `--continue` | `--continue` | Keep running independent tasks after a failure; skip tasks whose prerequisites failed; exit non-zero with a summary of how many failed. Default (omit the flag) is fail-fast: the first failure stops scheduling new work. |
| `--force` | `--force` | Ignore the cache for this run. Alias for `--no-cache` (see below). |
| `--no-cache` | `--no-cache` | Ignore the cache and re-run every task. **Equivalent to `--force`** — either one (or both) bypasses the cache. |
| `--dry-run` | `--dry-run` | List the tasks that would run, in dependency order, then exit without running them. |
| `"pipeline"` in `turbo.json` | `"pipeline"` in `lattice.json` | Task definitions with `dependsOn`, `inputs`, `outputs`, `env`. `^task` denotes a dependency's task; `task` denotes a same-workspace task. |

## Lattice extras (beyond Turbo's surface)

These have no direct Turbo analogue — they are part of Lattice's own voice:

- **`-l` / `--loquacious`** — a global flag that streams detailed, line-by-line
  task output, bypassing the interactive TUI. Handy for CI logs and debugging.
- Other subcommands: `setup` (run native dependency installers),
  `template` / `generate` (workspace scaffolding), and `version`.

The dev-binary hotswap for contributors working *on* Lattice is deliberately
**not** a subcommand — it lives as repo-local scripts (`scripts/dev-link.sh` /
`scripts/dev-unlink.sh`), keeping the shipped CLI free of self-hosting machinery
(PRD §13).

## `--force` vs `--no-cache`

They are **equivalent**. `--no-cache` is Lattice's native spelling; `--force` is
provided as a Turbo-familiar alias. Passing either (or both) causes the run to
ignore cached results and re-execute every task. Internally the two booleans are
OR'd together before reaching the runner.

## Deferred: bare-task shorthand (`lattice build`)

Turbo lets you write `turbo build` as shorthand for `turbo run build`. Lattice
**does not** ship this yet. With clap's derived subcommand enum, a clean
implementation that (a) never shadows real subcommands (`run`, `setup`,
`template`, `generate`, `version`, `--help`, `--version`) and (b) correctly
forwards every `run` flag (`--filter`,
`--concurrency`, `--continue`, `--force`, `--no-cache`, `--dry-run`) proved
fragile and ambiguous. Correctness was prioritized over completeness, so the
shorthand is deferred. Until then, use the explicit form:

```sh
lattice run build
lattice run test --filter api --concurrency 4 --continue
```
